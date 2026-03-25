# ICTL Core Semantics

This document defines the fundamental semantic rules of the Isolate Concurrent Temporal Language (ICTL).

---

## 1. Entropic Memory Model

The "Entropic" memory model is the cornerstone of ICTL. It treats values not as static data, but as energy states that evolve, move, and decay through computation.

### Value States
- **Valid**: The value is owned by the current branch and fully accessible.
- **Pending**: The value is a promise (e.g., from `defer`) that will resolve at a future `local_clock`.
- **Decayed**: The value (typically a struct) has had one or more of its fields consumed. The parent structure is no longer "sealed" and cannot be moved or sent as a whole.
- **Consumed**: The value has been destructively read (e.g., via `let x = y` or `chan_send`) and is no longer available in the arena.

### Rules of Consumption
1. **Move by Default**: Assignments (`let a = b`) move the ownership of `b` to `a`. `b` becomes `Consumed`.
2. **Structural Decay**: Accessing a field `s.f` consumes `f` and transitions `s` to a `Decayed` state.
3. **Explicit Cloning**: To reuse a value without consuming it, `clone(x)` must be used, which incurs a deterministic temporal cost.

---

## 2. Temporal Determinism

ICTL enforces strict, predictable execution time for all operations.

### Local Clock and Budget
- Every branch maintains a `local_clock` (measured in milliseconds).
- Every instruction has a deterministic cost (base cost: 1ms).
- Branches are initialized with a `cpu_budget_ms`. Exceeding this budget triggers a runtime `BudgetExhausted` fault.

### Pacing and Padding
- Constructs like `for` loops and `routine` calls enforce timing contracts.
- **Padding**: If a block of code completes faster than its contracted time (e.g., `taking 20ms`), the VM automatically pads the `local_clock` to match the contract, ensuring execution time is never source-dependent.
- **Watchdogs**: If a block exceeds its allocated time, a `WatchdogBite` is triggered, allowing recovery logic to intervene.

---

## 3. Timeline Isolation

Concurrency in ICTL is modeled as isolated **Timelines** (branches).

### Split and Merge
- **`split`**: Creates children that start with a snapshot of the parent's arena and clock.
- **`merge`**: Recombines children into a parent. Conflicts (two branches modifying/consuming the same global state) must be resolved via explicit `reconcile` or `resolving` rules.

---

## 4. Isochronous Scheduling

For high-precision timing, ICTL supports **Isochronous Matrix** scheduling.

- **Slices**: Using `slice Nms` sets a fixed tick rate.
- **Phase Commits**: `loop tick` blocks ensure that all channel operations happen in deterministic phases—sends are buffered until the tick boundary, and receives read from the previous tick's buffer.
