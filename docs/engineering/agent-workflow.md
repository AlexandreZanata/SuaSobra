# One-Microstep Agent Workflow

## Context and scope

Read [ADR-002](../decisions/0002-free-simple-launch.md), [ADR-003](../decisions/0003-rust-postgresql18.md), root AGENTS.md, the active task and its named domain rules. The active launch is a free, low-cost local connection service with no platform payments/returns. The old 101-step full-domain plan is archived privately; do not execute it or treat future-only rules as prerequisites.

Use one task at a time. Verify actual predecessor commits/evidence and preserve unrelated files. No agent delegation unless explicitly requested. A missing prerequisite is a blocker, not permission to invent architecture.

## Execution loop

1. Inspect status, branch, task progress and dependencies.
2. Read only relevant rules, define the expected persisted result and add meaningful success/denial/regression tests for changed behavior.
3. Implement the task's bounded work through the established service/contract boundary.
4. Run focused checks and necessary migration/docs checks; test real PostgreSQL effects, rollback, exact deadline and replay/permission where relevant. Missing required tools/tests/services are blockers.
5. Review the diff for secrets, unrelated changes, future features, weakened tests and protected-field bypasses.
6. Record actual commands, exit results, counts/skips, changed paths and limits in `.local/evidence/<TASK-ID>.md`.
7. Update public acceptance mapping and touched docs; stage explicit validated paths and commit `<TASK-ID>: <English description>`.
8. Record the commit and next task in private progress. Do not commit `.local/` or fabricate success.

Task statuses: pending, running, blocked, validated, committed. At a phase boundary, run its full local gate, confirm no unexplained deliverable edits, push its branch and verify the exact remote hash. A failed local gate/push blocks progression; no phase CI is required.

## Git and main integration

Remote at preparation: `origin`, `git@github.com:AlexandreZanata/SuaSobra.git`; the repository uses the owner-authorized clean Rust baseline on main. Inspect current remote/default history before establishing a base; never rewrite it without explicit owner authorization. Phase branches use `codex/phase-pNN` from the preceding delivered phase tip; inspect the actual branch history before choosing its base.

Each task gets a focused commit. At phase end use the actual branch name in `git push -u origin <branch>`, and verify `git ls-remote origin refs/heads/<branch>` matches its tip. Continue from that verified tip without waiting for CI. No force-push, automatic merge or deploy.

CI exists only at main integration: workflow `pull_request.branches: [main]`, with no phase push trigger or duplicate post-merge run. Main requires the successful stable check before merge through configured branch protection/rules. When requested to integrate, open the main-targeting PR, resolve failed checks and merge only with the actual authorization. If rules cannot be configured, record the limitation instead of claiming enforcement. [GitHub protected branch guidance](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).

## Verification commands

Foundation creates docs/lint/test-task/test-unit/test-integration/test-concurrency/migrations-check/verify targets. `test-task TEST_PATH=<Rust test target> TEST_FILTER=<optional test name>` runs the focused target/filter. Add test-contract/security/property/web/e2e/release targets only when their real suites exist; never turn a missing suite into a false pass.

Focused tests run per task; `make verify` and the phase's additional gates run at boundaries. CI reproduces these from committed inputs, with PostgreSQL and locks, never the private plan. Deferred financial/advanced acceptance definitions remain labeled deferred; active parts of mixed definitions still need assertions.

After two failed attempts on one substantive blocker, leave a minimal reproducer, exact error and next decision. Never weaken the rule/test or hide a required skip. Paid provisioning, DNS, destructive operations and production deploy require their actual authorization; phase push alone does not grant it.
