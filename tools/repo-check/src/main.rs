//! Repository documentation and private-plan tools, implemented entirely in Rust.
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn read_json(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}
fn string(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}
fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}
fn markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = vec![];
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(markdown_files(&path)?);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
fn finish(errors: Vec<String>, success: &str) -> Result<()> {
    if errors.is_empty() {
        println!("{success}");
        Ok(())
    } else {
        for error in errors {
            eprintln!("- {error}");
        }
        Err("Repository check failed".into())
    }
}
fn decode_path(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            output.push(value);
            i += 3;
            continue;
        }
        output.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}
fn links(path: &Path, errors: &mut Vec<String>) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let pattern = Regex::new(r"\[[^\]]+\]\(([^)]+)\)")?;
    let scheme = Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")?;
    for found in pattern.captures_iter(&content) {
        let target = found[1].trim().trim_matches(['<', '>']);
        if target.starts_with('#') || scheme.is_match(target) {
            continue;
        }
        let local = decode_path(target.split('#').next().unwrap_or_default());
        let candidate = path.parent().unwrap_or(Path::new(".")).join(&local);
        if !candidate.exists() {
            errors.push(format!("Broken link in {}: {target}", path.display()));
        }
    }
    Ok(())
}
fn docs(root: &Path, refresh: bool) -> Result<()> {
    let catalog_path = root.join("docs/domain/catalog.json");
    let mut catalog = read_json(&catalog_path)?;
    let mut errors = vec![];
    let mut definitions = BTreeMap::new();
    let definition = Regex::new(r"^(?:### |\- \*\*)((?:UC|INV|EDGE)-\d{3})")?;
    let ids = Regex::new(r"\b(?:UC|INV|EDGE)-\d{3}\b")?;
    let parts = array(&catalog["parts"]);
    if parts
        .iter()
        .map(|p| p["part"].as_u64().unwrap_or_default())
        .collect::<Vec<_>>()
        != (1..=17).collect::<Vec<_>>()
    {
        errors.push("Catalog must contain 17 ordered domain parts".into());
    }
    for part in parts {
        let relative = string(&part["path"]);
        let path = root.join(relative);
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => {
                errors.push(format!("Missing canonical file: {relative}"));
                continue;
            }
        };
        let heading = format!("## Part {} — {}\n", part["part"], string(&part["title"]));
        if !content.starts_with(&heading) {
            errors.push(format!("Unexpected part heading: {relative}"));
        }
        for (number, line) in content.lines().enumerate() {
            if let Some(found) = definition.captures(line) {
                let id = found[1].to_owned();
                if definitions
                    .insert(id.clone(), json!({"path":relative,"line":number+1}))
                    .is_some()
                {
                    errors.push(format!("Duplicate definition: {id}"));
                }
            }
        }
    }
    let expected: BTreeSet<String> = [("UC", 21), ("INV", 66), ("EDGE", 60)]
        .into_iter()
        .flat_map(|(prefix, count)| (1..=count).map(move |number| format!("{prefix}-{number:03}")))
        .collect();
    if definitions.keys().cloned().collect::<BTreeSet<_>>() != expected {
        errors.push("Domain definition IDs differ from the required 147 stable IDs".into());
    }
    let actual = json!(definitions);
    if refresh && errors.is_empty() {
        catalog["definitions"] = actual.clone();
        fs::write(
            catalog_path,
            format!("{}\n", serde_json::to_string_pretty(&catalog)?),
        )?;
    } else if catalog["definitions"] != actual {
        errors.push("Catalog locations are stale; run make docs-refresh".into());
    }
    let acceptance = read_json(root.join("docs/quality/acceptance-catalog.json"))?;
    let items = array(&acceptance["items"]);
    let covered: BTreeSet<String> = items
        .iter()
        .map(|item| string(&item["id"]).into())
        .collect();
    if items.len() != 147 || covered != expected {
        errors.push("Acceptance catalog must contain all 147 IDs exactly once".into());
    }
    for item in items {
        let id = string(&item["id"]);
        let status = string(&item["status"]);
        if item["source"] != actual[id]["path"] {
            errors.push(format!("Stale acceptance source: {id}"));
        }
        if !["planned", "implemented", "verified"].contains(&status) {
            errors.push(format!("Invalid coverage status: {id}"));
        }
        if ["implemented", "verified"].contains(&status) && array(&item["tests"]).is_empty() {
            errors.push(format!("Missing test mapping: {id}"));
        }
        for node in array(&item["tests"]) {
            let node = string(node);
            let Some((relative, function)) = node.split_once("::") else {
                errors.push(format!("Invalid Rust test node: {node}"));
                continue;
            };
            let source = fs::read_to_string(root.join(relative)).unwrap_or_default();
            let pattern = Regex::new(&format!(r"\bfn\s+{}\s*\(", regex::escape(function)))?;
            if !relative.ends_with(".rs") || !pattern.is_match(&source) {
                errors.push(format!("Missing mapped Rust test: {node}"));
            }
        }
        if status == "verified"
            && (item["verification"].is_null()
                || item["verification"] == false
                || item["verification"] == ""
                || item["verification"] == json!([])
                || item["verification"] == json!({}))
        {
            errors.push(format!("Missing verification evidence: {id}"));
        }
    }
    let mut paths = vec![root.join("README.md"), root.join("AGENTS.md")];
    paths.extend(markdown_files(&root.join("docs"))?);
    let code = Regex::new(r"(?m)^```(?:rust|python|javascript|typescript|sql)")?;
    for path in paths {
        let content = fs::read_to_string(&path)?;
        for found in ids.find_iter(&content) {
            if !expected.contains(found.as_str()) {
                errors.push(format!(
                    "Unknown domain reference {}: {}",
                    found.as_str(),
                    path.display()
                ));
            }
        }
        if !content.ends_with('\n') {
            errors.push(format!("Missing final newline: {}", path.display()));
        }
        links(&path, &mut errors)?;
        if path.starts_with(root.join("docs/domain")) && code.is_match(&content) {
            errors.push(format!(
                "Implementation code in business domain: {}",
                path.display()
            ));
        }
    }
    finish(
        errors,
        "Documentation valid: 17 parts, 147 stable definitions, accurate locations, acceptance statuses, test nodes and local links.",
    )?;
    println!("Application verification is not implied by a documentation check.");
    Ok(())
}

#[cfg(test)]
mod tests;
fn plan(root: &Path) -> Result<()> {
    let base = root.join(".local/plan");
    let manifest = read_json(base.join("manifest.json"))?;
    let progress = read_json(root.join(".local/progress/status.json"))?;
    let catalog = read_json(root.join("docs/domain/catalog.json"))?;
    let mut errors = vec![];
    let mut prior = BTreeSet::new();
    let mut count = 0;
    for phase in array(&manifest["phases"]) {
        let phase_id = string(&phase["id"]);
        if !base.join(string(&phase["path"])).is_file() {
            errors.push(format!("Missing phase file: {phase_id}"));
        }
        for task in array(&phase["tasks"]) {
            count += 1;
            let id = string(&task["id"]);
            let content = fs::read_to_string(base.join(string(&task["path"]))).unwrap_or_default();
            if !content.starts_with(&format!("# {id} — {}\n", string(&task["title"]))) {
                errors.push(format!("Missing task or mismatched title: {id}"));
            }
            for heading in [
                "Objective and prerequisite",
                "Read this context only",
                "Allowed change paths",
                "Concrete work",
                "Acceptance checks",
                "Required verification",
                "Exclusions and stop conditions",
                "Commit and handoff",
            ] {
                if !content.contains(&format!("## {heading}")) {
                    errors.push(format!("Missing {heading}: {id}"));
                }
            }
            for dependency in array(&task["depends_on"]) {
                if !prior.contains(string(dependency)) {
                    errors.push(format!(
                        "Invalid/non-prior dependency {}: {id}",
                        string(dependency)
                    ));
                }
            }
            for rule in array(&task["rules"]) {
                if catalog["definitions"][string(rule)].is_null() {
                    errors.push(format!("Unknown rule {}: {id}", string(rule)));
                }
            }
            let state = &progress["tasks"][id];
            if !["pending", "running", "blocked", "validated", "committed"]
                .contains(&string(&state["status"]))
            {
                errors.push(format!("Invalid task status: {id}"));
            }
            if state["status"] == "committed"
                && (string(&state["commit"]).is_empty() || string(&state["evidence"]).is_empty())
            {
                errors.push(format!("Committed task missing hash/evidence: {id}"));
            }
            if !prior.insert(id.to_owned()) {
                errors.push(format!("Duplicate task ID: {id}"));
            }
        }
        let state = &progress["phases"][phase_id];
        if state["status"] == "complete" {
            if string(&state["remote_revision"]).is_empty() || string(&state["ci"]).is_empty() {
                errors.push(format!(
                    "Complete phase lacks delivery evidence: {phase_id}"
                ));
            }
            if array(&phase["tasks"])
                .iter()
                .any(|task| progress["tasks"][string(&task["id"])]["status"] != "committed")
            {
                errors.push(format!("Complete phase has uncommitted tasks: {phase_id}"));
            }
        }
    }
    for path in markdown_files(&base)? {
        links(&path, &mut errors)?;
    }
    let ignored = Command::new("git")
        .args(["check-ignore", ".local/plan/manifest.json"])
        .current_dir(root)
        .output()?;
    if !ignored.status.success() {
        errors.push("Private plan is not ignored".into());
    }
    let tracked = Command::new("git")
        .args(["ls-files", ".local"])
        .current_dir(root)
        .output()?;
    if !tracked.status.success() || !tracked.stdout.is_empty() {
        errors.push("Private files are tracked or privacy could not be checked".into());
    }
    finish(
        errors,
        &format!(
            "Plan valid: {} phases, {count} tasks, ordered dependencies, known rules and ignored private files.",
            array(&manifest["phases"]).len()
        ),
    )?;
    println!("Plan validation does not imply tests, commits, delivery or production readiness.");
    Ok(())
}
fn next(root: &Path) -> Result<()> {
    let manifest = read_json(root.join(".local/plan/manifest.json"))?;
    let progress = read_json(root.join(".local/progress/status.json"))?;
    let mut previous = None;
    for phase in array(&manifest["phases"]) {
        if let Some(id) = previous
            && progress["phases"][id]["status"] != "complete"
        {
            println!(
                "Blocked: previous phase {id} needs its local gate, push and verified remote revision; no phase CI."
            );
            return Ok(());
        }
        for task in array(&phase["tasks"]) {
            let id = string(&task["id"]);
            let state = &progress["tasks"][id];
            if state["status"] != "committed" {
                for dependency in array(&task["depends_on"]) {
                    if progress["tasks"][string(dependency)]["status"] != "committed" {
                        return Err(format!("Blocked dependency: {}", string(dependency)).into());
                    }
                }
                println!(
                    "Next task: {id} — {}\nPath: .local/plan/{}\nStatus: {}",
                    string(&task["title"]),
                    string(&task["path"]),
                    string(&state["status"])
                );
                return Ok(());
            }
        }
        previous = Some(string(&phase["id"]));
    }
    if let Some(id) = previous
        && progress["phases"][id]["status"] != "complete"
    {
        println!("Final phase {id} still needs local gates and verified delivery.");
    } else {
        println!(
            "All private phase states are complete; inspect actual evidence before claiming readiness."
        );
    }
    Ok(())
}
fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("Missing repository root")?;
    let arguments: Vec<_> = env::args().skip(1).collect();
    match arguments.first().map(String::as_str).unwrap_or("docs") {
        "docs" => docs(
            root,
            arguments.iter().any(|argument| argument == "--refresh"),
        ),
        "plan" => plan(root),
        "next" => next(root),
        _ => Err("Usage: repo-check [docs [--refresh]|plan|next]".into()),
    }
}
