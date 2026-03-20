# ICTL Agent Recommendations

This project uses a workspace Copilot instruction file in `.github/copilot-instructions.md`.

## Suggested agents / modes
- `agent-customization` skill: For writing or refining instructions, prompts, and workflows in this repo.
- `default` agent: For general code changes and implementation tasks.

## Workflow
1. Read `.github/copilot-instructions.md` first.
2. Use `README.md` and `CONTRIBUTING.md` for project conventions.
3. Prefer small runs of `cargo test` and `cargo fmt` in every PR.

## Templates
- Parser change: update `src/frontend/ictl.pest`, `src/frontend/parser.rs`, add parser unit tests
- Analyzer change: update `src/analysis/analyzer.rs`, add semantic tests
- VM/memory change: update `src/runtime/{vm.rs,memory.rs}`, add temporal regression tests
