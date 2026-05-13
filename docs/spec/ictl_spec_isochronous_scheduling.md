# Isochronous Matrix Scheduling (Slice and Tick)

This document formalizes the isochronous scheduling model within the Isolate Concurrent Temporal Language (ICTL), engineered for high-precision, fixed-rate iterative execution.

---

## 1. Theoretical Motivation

Standard execution within ICTL is relative and asynchronous. The **Isochronous Matrix** model introduces fixed-duration temporal windows (slices) to facilitate synchronous data processing and deterministic pipeline stages.

---

## 2. Fixed-Slice Scheduling Protocol (`slice`)

The `slice` statement specifies a constant temporal window for all subsequent `loop tick` operations within a designated isolate.

### Formal Syntax
```ictl
isolate [<identifier>] {
    slice <amount>ms
    <statements>
}
```

**Execution Semantics:**
- **State Binding**: The `slice <amount>ms` declaration binds the isolate's `cpu_budget_ms` and `slice_ms` within the timeline state.
- **Invariant Enforcement**: Every `loop tick` within the isolate must complete its execution within the specified `slice_ms` duration.

---

## 3. Tick-Based Iteration (`loop tick`)

The `loop tick` construct defines a single iteration within a fixed-duration loop.

### Formal Syntax
```ictl
loop tick {
    <statements>
    [break]
}
```

**Operational Logic:**
1. **Body Execution**: Statements within the `loop tick` are executed according to standard semantic rules.
2. **Deterministic Padding**: Upon completion of the body (or execution of a `break`), the Stack-based Temporal Virtual Machine (STVM) automatically advances the `local_clock` to the comprehensive `slice_ms` duration.
3. **Phase-Committed Communication**: Channel operations within a tick utilize a **double-buffered phase** model:
   - **Transmission Phase**: `chan_send` operations write to a `pending` buffer.
   - **Boundary Commit**: At the conclusion of the tick, all `pending` buffers are committed to the active channels.
   - **Reception Phase**: `chan_recv` operations read exclusively from data committed during the **preceding** tick.

---

## 4. Deterministic Pipeline Architectures

The isochronous model facilitates the implementation of reliable scatter-gather and pipelined execution architectures:
- **Tick N**: The producer transmits data to a designated channel.
- **Tick N+1**: The consumer retrieves the data produced during Tick N.

This formal separation of production and consumption phases eliminates race conditions and ensures that data processing timing is entirely independent of the underlying hardware execution velocity.

---

## 5. Foundational Constraint Rules

1. **Slice Context Requirement**: `loop tick` operations are permitted only within an `isolate` block containing an active `slice` declaration.
2. **WCET Compliance**: The Worst-Case Execution Time (WCET) of the `loop tick` body must not exceed the specified `slice_ms`. Violations result in a `WatchdogBite` trigger.
