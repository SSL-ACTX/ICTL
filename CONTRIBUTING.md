# Contributing to ICTL

Thank you for contributing. Please follow these guidelines.

## Workflow
1. Open issues for feature requests or bugs.
2. Create a feature branch: `git checkout -b feat/<summary>`.
3. Make small, focused commits; prefer one feature per PR.
4. Add a targeted test case for every behavior change.
5. Run:
   - `cargo test`
   - `cargo fmt`

## Testing
- Unit tests live in `src/` and integration tests in `tests/`.
- Keep tests minimal and deterministic.
- Use the same small sample programs that demonstrate the behavior.

## Commits
- Use Conventional Commits:
  - `feat:` for new language constructs.
  - `fix:` for bugfixes.
  - `chore:` for tooling/docs.
  - `test:` for adding tests.

## Code structure
- `src/frontend`: parser + AST
- `src/analysis`: entropic static checks
- `src/runtime`: VM + memory model
- `src/main.rs`: sample demo runner
- `src/lib.rs`: library interface for tests and embedding

## Review
- Ensure static analyzer invariants are preserved.
- Ensure `split` / `merge` memory semantics are unchanged unless intentional.
- Document temporal costs and branch interactions in comments.
