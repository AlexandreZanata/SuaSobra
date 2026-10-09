use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = env::temp_dir().join(format!(
            "suasobra-repo-check-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("docs/domain")).unwrap();
        fs::create_dir_all(root.join("docs/quality")).unwrap();
        fs::create_dir_all(root.join(".local/plan/tasks")).unwrap();
        fs::create_dir_all(root.join(".local/progress")).unwrap();
        let fixture = Self(root);
        fixture.write("README.md", "# Example\n");
        fixture.write("AGENTS.md", "# Instructions\n");
        fixture.write(".gitignore", "/.local/\n");
        let mut parts = vec![];
        let mut definitions = BTreeMap::new();
        let mut items = vec![];
        for part in 1..=17 {
            let path = format!("docs/domain/part-{part}.md");
            let mut text = format!("## Part {part} — Example\n");
            if part == 1 {
                for (prefix, count) in [("UC", 21), ("INV", 66), ("EDGE", 60)] {
                    for number in 1..=count {
                        let id = format!("{prefix}-{number:03}");
                        definitions.insert(
                            id.clone(),
                            json!({"path":path,"line":text.lines().count()+1}),
                        );
                        text.push_str(&format!("### {id} Example\n"));
                        items.push(json!({"id":id,"source":path,"status":"planned","tests":[]}));
                    }
                }
            }
            fixture.write(&path, &text);
            parts.push(json!({"part":part,"title":"Example","path":path}));
        }
        fixture.json(
            "docs/domain/catalog.json",
            json!({"parts":parts,"definitions":definitions}),
        );
        fixture.json(
            "docs/quality/acceptance-catalog.json",
            json!({"items":items}),
        );
        fixture.write(".local/plan/phase.md", "# Phase\n");
        let headings = [
            "Objective and prerequisite",
            "Read this context only",
            "Allowed change paths",
            "Concrete work",
            "Acceptance checks",
            "Required verification",
            "Exclusions and stop conditions",
            "Commit and handoff",
        ];
        fixture.write(
            ".local/plan/tasks/task.md",
            &format!(
                "# TEST-001 — Example\n{}",
                headings.map(|heading| format!("\n## {heading}\n")).join("")
            ),
        );
        fixture.json(".local/plan/manifest.json", json!({"phases":[{"id":"P00","path":"phase.md","tasks":[{"id":"TEST-001","title":"Example","path":"tasks/task.md","depends_on":[],"rules":["INV-001"]}]}]}));
        fixture.json(".local/progress/status.json", json!({"tasks":{"TEST-001":{"status":"pending"}},"phases":{"P00":{"status":"pending"}}}));
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&fixture.0)
                .status()
                .unwrap()
                .success()
        );
        fixture
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.0.join(path), text).unwrap();
    }
    fn json(&self, path: &str, value: Value) {
        self.write(
            path,
            &format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
        );
    }
    fn change_json(&self, path: &str, update: impl FnOnce(&mut Value)) {
        let mut value = read_json(self.0.join(path)).unwrap();
        update(&mut value);
        self.json(path, value);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn valid_documentation_and_private_plan_pass() {
    let fixture = Fixture::new();
    docs(&fixture.0, false).unwrap();
    plan(&fixture.0).unwrap();
    next(&fixture.0).unwrap();
}

#[test]
fn moved_definitions_require_refresh_and_preserve_ids() {
    let fixture = Fixture::new();
    let path = fixture.0.join("docs/domain/part-1.md");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(path, original.replacen('\n', "\n\n", 1)).unwrap();
    assert!(docs(&fixture.0, false).is_err());
    docs(&fixture.0, true).unwrap();
    docs(&fixture.0, false).unwrap();
    assert_eq!(
        read_json(fixture.0.join("docs/domain/catalog.json")).unwrap()["definitions"]["UC-001"]["line"],
        3
    );
}

#[test]
fn refresh_cannot_hide_missing_or_duplicate_definitions() {
    for replacement in ["### UC-002 Example\n", "Example\n"] {
        let fixture = Fixture::new();
        let path = fixture.0.join("docs/domain/part-1.md");
        let original = fs::read_to_string(&path).unwrap();
        fs::write(path, original.replace("### UC-001 Example\n", replacement)).unwrap();
        let catalog = fs::read(fixture.0.join("docs/domain/catalog.json")).unwrap();
        assert!(docs(&fixture.0, true).is_err());
        assert_eq!(
            fs::read(fixture.0.join("docs/domain/catalog.json")).unwrap(),
            catalog
        );
    }
}

#[test]
fn acceptance_requires_real_mapped_rust_function_and_evidence() {
    let fixture = Fixture::new();
    fixture.change_json("docs/quality/acceptance-catalog.json", |value| {
        value["items"][0]["status"] = json!("implemented");
        value["items"][0]["tests"] = json!(["test.rs::scenario"]);
    });
    assert!(docs(&fixture.0, false).is_err());
    fixture.write("test.rs", "#[test]\nfn unrelated() {}\n");
    assert!(docs(&fixture.0, false).is_err());
    fixture.write("test.rs", "#[test]\nfn scenario() {}\n");
    docs(&fixture.0, false).unwrap();
    fixture.change_json("docs/quality/acceptance-catalog.json", |value| {
        value["items"][0]["status"] = json!("verified");
        value["items"][0]["verification"] = json!("");
    });
    assert!(docs(&fixture.0, false).is_err());
    fixture.change_json("docs/quality/acceptance-catalog.json", |value| {
        value["items"][0]["verification"] = json!("Recorded evidence")
    });
    docs(&fixture.0, false).unwrap();
}

#[test]
fn duplicate_acceptance_items_and_invalid_status_fail() {
    let fixture = Fixture::new();
    fixture.change_json("docs/quality/acceptance-catalog.json", |value| {
        value["items"][0] = value["items"][1].clone()
    });
    assert!(docs(&fixture.0, false).is_err());
    let fixture = Fixture::new();
    fixture.change_json("docs/quality/acceptance-catalog.json", |value| {
        value["items"][0]["status"] = json!("done")
    });
    assert!(docs(&fixture.0, false).is_err());
}

#[test]
fn broken_links_unknown_ids_and_domain_code_fail() {
    for content in [
        "[Missing](missing.md)\n",
        "Unknown INV-999\n",
        "No final newline",
    ] {
        let fixture = Fixture::new();
        fixture.write("README.md", content);
        assert!(docs(&fixture.0, false).is_err());
    }
    let fixture = Fixture::new();
    fixture.write(
        "docs/domain/extra.md",
        "```rust\nfn implementation() {}\n```\n",
    );
    assert!(docs(&fixture.0, false).is_err());
}

#[test]
fn percent_encoded_local_links_and_external_links_pass() {
    let fixture = Fixture::new();
    fixture.write("with space.md", "Example\n");
    fixture.write(
        "README.md",
        "[File](with%20space.md#anchor) [Web](https://example.com/missing) [Section](#anchor)\n",
    );
    docs(&fixture.0, false).unwrap();
}

#[test]
fn private_plan_rejects_invalid_dependency_and_blocks_next() {
    let fixture = Fixture::new();
    fixture.change_json(".local/plan/manifest.json", |value| {
        value["phases"][0]["tasks"][0]["depends_on"] = json!(["MISSING"])
    });
    assert!(plan(&fixture.0).is_err());
    assert!(next(&fixture.0).is_err());
}

#[test]
fn private_plan_rejects_unproven_completion_and_missing_context() {
    let fixture = Fixture::new();
    fixture.change_json(".local/progress/status.json", |value| {
        value["tasks"]["TEST-001"]["status"] = json!("committed");
        value["phases"]["P00"]["status"] = json!("complete");
    });
    assert!(plan(&fixture.0).is_err());
    let fixture = Fixture::new();
    fixture.write(".local/plan/tasks/task.md", "# TEST-001 — Example\n");
    assert!(plan(&fixture.0).is_err());
}

#[test]
fn private_plan_must_be_ignored_and_never_tracked() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", "\n");
    assert!(plan(&fixture.0).is_err());
    fixture.write(".gitignore", "/.local/\n");
    assert!(
        Command::new("git")
            .args(["add", "--force", ".local/plan/manifest.json"])
            .current_dir(&fixture.0)
            .status()
            .unwrap()
            .success()
    );
    assert!(plan(&fixture.0).is_err());
}
