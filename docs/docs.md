# ICTL Language Documentation

This is the top-level language documentation landing page.

## Table of contents

- [Overview](lang/overview.md)
- [Core semantics](lang/semantics.md)
- [Syntax reference](lang/syntax.md)
- [Paced iteration / split_map](lang/iter_split_map.md)
- [Conditional reconciliation (`if` / `else`)](lang/if_else.md)
- [Speculative branches (`speculate` / `fallback` / `collapse`)](lang/speculative_branches.md)
- [Advanced routing (`select` / `match entropy`)](lang/advanced_routing.md)

## How to use

Browse the docs in `docs/lang/` for detailed syntax, semantics, and examples.

These docs are maintained to align with the runtime implementation in `src/runtime/vm.rs`, the analyzer rules in `src/analysis/analyzer.rs`, and parser/AST in `src/frontend`.
