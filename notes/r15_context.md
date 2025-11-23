# R15 working context (ah task CLI parity, browser automation out of scope)

## Current branch

- `feat/r15`

## Key specs/docs

- `specs/Public/R15.status.md` — milestone/verification checklist (M1 done, M2 done, M3–M5 pending)
- `specs/Public/CLI.md` — `ah task` spec; browser automation explicitly excluded for R15
- `specs/Public/Scenario-Format.md` — scenario YAML structure
- `AGENTS.md` — repo-specific contributing/testing notes

## Recent work (done)

- M1 verifications: CLI parsing/agent flag precedence, config precedence/provenance, help snapshot.
- M2 verifications: prompt processing (template stripping, comment-prefix handling, empty detection), branch validation (regex, primary guard, follow-up detection), metadata commit message and `.agents/tasks` content.
- Scenario coverage added (tests/tools/mock-agent/scenarios):
  - `task_create_new_branch.yaml`
  - `task_follow_up_append.yaml`
  - `task_metadata_only_commit.yaml`
  - `task_push_behaviors.yaml`
- Scenario harness: `crates/ah-cli/tests/task_scenario_tests.rs` (runs against `CARGO_BIN_EXE_ah`, serialized for DB isolation, git remotes seeded).
- Branch handling fix: when task files disabled but metadata commits enabled, still create/validate branch; existing-branch validation also runs when skipping task files.

## Tests recently green

- `cargo test -p ah-cli --lib -- --nocapture`
- `cargo test -p ah-cli --test task_scenario_tests -- --nocapture`
  (Full `just test-rust` not re-run after latest changes.)

## Outstanding (M3–M5, per R15.status.md)

- M3: execution planning/workspace selection via `prepare_workspace_with_fallback`; wire to `TaskManager`/`AgentTasks`/`local_task_manager`; guard cloud/browser paths; multi-agent/fleet orchestration; `--follow` UI hand-off; integration/scenario tests for local/remote paths.
- M4: delivery modes `branch/pr/patch`, push gating, repo sanity checks/cleanup, notifications propagation; integration/unit + scenario tests.
- M5: docs/help sync (CLI.md), note browser automation out of scope; `just manual-test-task`; full lint/test runs; doc spell/link checks as needed.

## Notes/gotchas

- DB isolation: tests use `reset_ah_home`; avoid cross-test reuse.
- Comment prefix resolution honors git `core.commentString` unless config disables (`task-editor.use-vcs-comment-string`).
- Task files are timestamped `.agents/tasks/YYYY/MM/<day>-<HHMM>-<branch>`.
- Follow-up detection uses `AgentTasks::on_task_branch`; commit adds `--- FOLLOW UP TASK ---`.
- Push handling uses `PushHandler`; non-interactive requires explicit `--push-to-remote` or `--yes`.
- Browser automation paths must error with clear “disabled this release” messaging.
