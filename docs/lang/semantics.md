# ICTL Semantics

## Entropic Memory

ICTL tracks value state explicitly:

- `let x = y` moves from `y` (if `y` is already owned) unless using `clone`.
- `consume` is destructive: `branch.arena.consume(name)` in VM removes value and marks it consumed.
- `clone` in analysis maps to clone cost and unique copy semantics.
- `set_consumed` is used for reconciliation on paranetic operations.
- field access (e.g., `a.b`) causes structural decay in parent, meaning parent is now partially consumed (decayed). Direct parent reuse may be rejected.

## Timeline and time model

- Each timeline (`Timeline`) has:
  - `local_clock` (ms time visited)
  - `cpu_budget_ms` (available CPU ticks)
  - `arena` (entropic values states)
- `split` creates child timelines by cloning parent state.
- `merge` recombines child semantics into target based on explicit resolutions.
- `if` / `else` performs speculative path evaluation and then merges final state.

## Temporal Flow

- Each statement increments local clock by default (1ms), except:
  - `network_request` costs 5ms
  - `split`, `merge`, `watchdog`, `if` extra internal costs
- `for` and `split_map` enforce pacing budgets and max bounds.

## Watchdogs and anchors

- `anchor` saves a backup of `local_clock` and `arena`.
- `rewind_to` resets timeline to anchor state (with chaos guard).
- `watchdog` monitors and executes recovery block on timeout then returns error `WatchdogBite`.

## Speculation (`speculate` / `fallback` / `collapse`)

- `speculate (max Nms) { ... } fallback { ... }` creates a temporary micro-timeline.
- `speculation_mode(selective|full)` sets how successful speculative commits are merged (selective default).
- Speculative body runs isolated using cloned arena and local_clock.
- `commit` inside speculative body marks success; `collapse` or exceeded max causes failure and fallback.
- On success, the parent branch can merge full child state or selective commit fields depending on runtime mode (`SpeculationCommitMode`).
- `select (max Nms) { ... }` performs bounded channel race with deterministic padding and optional `timeout` path.
- `match entropy(x) { ... }` routes based on value state (`Valid` / `Decayed` / `Consumed`).
- VM time is padded to `max Nms + fallback_wcet` for deterministic cost.

## Notes for implementers

- Analyzer static rules in `src/analysis/analyzer.rs` check cross-branch consumptive consistency.
- Runtime rules (described in `src/runtime/vm.rs`) create action-level faults for pacing, consumption, and branch resolution.
- Use tests in `tests/integration.rs` as pattern:
  - `integration_if_requires_reconcile_for_crosspath_consume`
  - `integration_for_loop_pacing_and_bounds`
  - `integration_split_map_collects_yields`
