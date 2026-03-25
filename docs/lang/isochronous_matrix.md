# Isochronous Matrix (Slice + Tick)

This document formalizes the isochronous scheduling model in ICTL, designed for high-precision, fixed-rate execution loops.

---

## 1. Core Motivation

Standard ICTL execution is relative and asynchronous. The **Isochronous Matrix** model introduces fixed-time windows (slices) to enable synchronous data processing and deterministic pipeline stages.

---

## 2. Fixed-Slice Scheduling (`slice`)

The `slice` statement defines a constant temporal window for all subsequent `loop tick` operations within an isolate.

### Syntax
```ictl
isolate [<identifier>] {
    slice <amount>ms
    <statements>
}
```

**Semantics:**
- **Binding**: The `slice <amount>ms` declaration binds the isolate's `cpu_budget_ms` and `slice_ms` in the timeline state.
- **Enforcement**: Any `loop tick` within this isolate must fit its execution within the specified `slice_ms`.

---

## 3. Tick Execution (`loop tick`)

The `loop tick` construct defines a single iteration of a fixed-time loop.

### Syntax
```ictl
loop tick {
    <statements>
    [break]
}
```

**Behavior:**
1. **Body Execution**: The statements inside the `loop tick` are executed normally.
2. **Padding**: After the body (or a `break`), the VM automatically advances the `local_clock` to the full `slice_ms` duration.
3. **Phase-Commit**: Channel operations inside a tick follow a **double-buffered phase** model:
   - **Sends**: `chan_send` writes to a `pending` buffer.
   - **Boundary**: At the end of the tick, all `pending` buffers are committed to the live `channels`.
   - **Receives**: `chan_recv` reads from the data committed in the **previous** tick.

---

## 4. Deterministic Pipelines

The isochronous model enables reliable scatter-gather and pipeline architectures:
- **Tick N**: Producer sends data to a channel.
- **Tick N+1**: Consumer receives the data produced in Tick N.

This separation of production and consumption phases eliminates race conditions and ensures that the timing of data processing is completely independent of the actual execution speed of the underlying hardware.

---

## 5. Constraint Rules

1. **Active Slice Required**: `loop tick` can only be used within an `isolate` block that has an active `slice` declaration.
2. **WCET Compliance**: The Worst-Case Execution Time (WCET) of the `loop tick` body must not exceed the `slice_ms`. Overruns result in a `WatchdogBite`.
