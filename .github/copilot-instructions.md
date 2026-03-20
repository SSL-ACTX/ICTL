# GitHub Copilot Instructions for ICTL

## Project purpose
ICTL is a research-focused Rust project implementing a temporal concurrency language and VM using:
- AST/parser (`src/frontend/ast.rs`, `src/frontend/parser.rs`, `src/frontend/ictl.pest`)
- Analyzer (`src/analysis/analyzer.rs`) for entropic/causal checks
- VM (`src/runtime/vm.rs`) with timeline budgeting
- Memory model (`src/runtime/memory.rs`) for entropy semantics

The repo is small and self-contained; aim to preserve trust in the existing semantics while improving correctness and readability.

## High-level conventions
- Use idiomatic Rust (edition 2021)
- Prefer explicit error mapping and `anyhow` for user-facing diagnostics
- Avoid unsafe unless absolute performance/embedding necessity
- Keep branch complexity low in parser/analyzer to prevent semantics drift

## Build, run, test commands
- `cargo build`
- `cargo run`
- `cargo test -- --nocapture`
- `cargo fmt`

## Main workflow for code changes
1. Locate intent in `README.md` (core concept) -> code path in `src/`
2. Check parser, analyzer, VM for consistency in semantics
3. Add regression tests in `src` (currently crate-integrated via unit tests in source files)
4. Run `cargo test` and format with `cargo fmt`

## Key source files
- `src/frontend/ast.rs`, `src/frontend/parser.rs`, `src/frontend/ictl.pest`: language syntax and parser
- `src/analysis/analyzer.rs`: static/entropic checks
- `src/runtime/vm.rs`: runtime execution semantics
- `src/runtime/memory.rs`: memory arena and decay semantics
- `src/main.rs`: CLI entrypoint
- `src/lib.rs`: library entrypoint for embedding and tests

## Documentation links
- README: https://github.com/SSL-ACTX/ictl/blob/main/README.md
- RFCs: `docs/RFC.md` for spec/math and evolution notes
- Always check RFC (if available) when implementing syntax or semantics changes.
- Do not introduce syntax or behavior that diverges from the RFC without a documented design proposal and team alignment.

## PR Quality Guidance
- Keep behavior changes minimal and semantics-preserving unless explicitly requested.
- Add focused tests for every feature or bugfix (TDD-style), small and precise.
- Add regression tests for fixed edge cases.
- Document invariants in comments for explicit semantics (split/merge, guardian patterns, clock costs).
- Track temporal effects in both analyzer and VM layers.
- Use Conventional Commits for all branch and PR history (feat/fix/docs/refactor/test/chore).

## Mandatory checks before merge
1. `cargo fmt` passes.
2. `cargo test` passes.
3. New test coverage validating the core semantic impact is included.
4. `README.md` or `CONTRIBUTING.md` gets updates when APIs change.

## Useful first tasks for Copilot interactions
- Add or fix parser rules for timeline qualifiers or time literals
- Implement missing analyzer checks for decay and aliasing
- Fix conflict-resolve strategy in merge
- Add logging / diagnostics around the `watchdog` path

## Prompting examples
- "In ICTL, add a parser rule for `@timeline:` labeled blocks and write a unit test."
- "Update the analyzer so field access breaks parent struct seal and add coverage."
- "Implement deterministic 1ms clock advancement semantics in `src/vm.rs` around `split`/`merge`."

## Apply-to recommendations
- For parser-specific changes, apply to `src/parser.rs`, `src/ictl.pest`
- For semantic checks, apply to `src/analyzer.rs`
- For runtime semantics, apply to `src/vm.rs`, `src/memory.rs`

> Keep these instructions concise and rely on `README.md` for full conceptual spec.  
> Avoid duplicating large design text; link to canonical docs.
