# ICTL Agent-Customization Skill

## Purpose
Provide a standard approach for building and iterating ICTL code changes in this repository, focused on parser/analyzer/vm semantics and entropic temporal correctness.

## When to use
- Adding new language grammar constructs
- Fixing static entropic checks
- Changing timeline merge or decay behavior
- Writing or expanding runtime VM semantics

## Workflow (from this conversation)
1. Discover existing conventions
   - Search for existing agent customizations and docs
   - Keep original content; extend where needed
2. Explore codebase
   - Identify core files (`src/parser.rs`, `src/ictl.pest`, `src/analyzer.rs`, `src/vm.rs`, `src/memory.rs`)
   - Gather build/test commands from `README.md`
3. Generate or update customization docs
   - Create/update `.github/copilot-instructions.md` with project purpose, conventions, build/test commands, workflow, prompt examples
   - Link to README and RFC, avoid duplication
   - Include testing and commit norms
4. Iterate
   - Ask for clarification earlier rather than rework later
   - Add small targeted content first, then refine

## Quality criteria
- One feature => one focused test (TDD-style)
- Tests should be minimal and precise
- Preserve invariants in parser/analyzer/VM
- keep branches simple and robust for static analysis
- Use Conventional Commits in every PR

## Output
A saved `SKILL.md` plus a `.github/copilot-instructions.md` that is maintained, with:
- template sections (purpose, conventions, commands, workflow)
- explicit state on source modules and expected behaviors

## Example prompts
- "Using the ICTL skill, add support for `@timeline:` blocks and include parser + analyzer tests."
- "In ICTL, implement deterministic 1ms cost for `split`/`merge` and add a regression test."
- "Update watchdog recovery path in `src/vm.rs` and write a focused test." 

## Next customization ideas
- `agent-customization/create-prompt.md`: standard prompt templates for common changes
- `agent-customization/create-hook.md`: pre-commit hook instructions for `cargo fmt && cargo test`
