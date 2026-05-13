# Paced Iteration and Parallelism

This document specifies the formal iterative constructs within the Isolate Concurrent Temporal Language (ICTL) and their deterministic temporal and entropic execution semantics.

---

## 1. Paced Sequential Iteration (`for`)

The `for` construct is engineered for sequential iteration with fixed-duration execution for each element in a collection.

### Formal Syntax
```ictl
for <item> <mode> <source> [pacing <amount>ms] [(max <amount>ms)] {
  <statements>
}
```

**Execution Modes:**
- **`consume`**: The `<source>` collection undergoes destructive read from the memory arena. Each `<item>` is moved into the loop's local scope.
- **`clone`**: The `<source>` collection remains in its current state. Each `<item>` is a deep replication of the original element, incurring a deterministic temporal cost.

**Temporal Pacing (`pacing <N>ms`):**
- Enforces an exact duration of `Nms` for each iteration of the loop body.
- If the body executes in less than `Nms`, the `local_clock` is **padded** to meet the requirement.
- If the body exceeds `Nms`, a `WatchdogBite` is triggered.

**Execution Budget (`max <N>ms`):**
- Specifies the total temporal boundary for the entire loop execution. If the loop completes prior to the `max` budget, the remaining duration is added to the `local_clock`.

---

## 2. Fixed-Frequency Iteration (`loop`)

The `loop` statement provides a mechanism for the repeated execution of a logic block with a deterministic periodicity.

### Formal Syntax
```ictl
loop (max <amount>ms) {
  <statements>
  [break]
}
```

### Isochronous Task Execution (`loop tick`)
The `loop tick` is a specialized iterative construct designed for isochronous operations.

```ictl
loop tick {
  <statements>
  [break]
}
```

- **Isochronous Synchronization**: Each `tick` is synchronized to a fixed temporal slice (e.g., `1ms` or `10ms`) as defined by the active `slice` context.
- **Phase-Committed State**: Internal state modifications are committed at the conclusion of each tick boundary.

---

## 3. Scatter-Gather Parallelism (`split_map`)

The `split_map` primitive implements a deterministic parallel mapping architecture.

### Formal Syntax
```ictl
split_map <item> <mode> <source> {
  <statements>
  [yield <expression>]
} [reconcile (<resolution_rules>)]
```

**Execution Semantics:**
1. **Parallel Timeline Initialization**: A child timeline is initialized for each element within the `<source>` collection.
2. **Snapshot Initialization**: Each child timeline begins execution with a snapshot of the parent's state.
3. **Timeline Isolation**: Child timelines execute the logic block independently and concurrently.
4. **Data Aggregation**: Values emitted via the `yield` primitive in each child are aggregated into a specialized `splitmap_results` array within the parent timeline.

### Reconciliation Protocols
Conflicts arising from concurrent modifications of shared variables (cloned from the parent) are resolved through formal `reconcile` rules.

---

## 4. Iterative Construct Comparative Analysis

| Primitive       | Execution Model | Memory Arena Impact     | Primary Use Case                                    |
| :-------------- | :-------------- | :---------------------- | :-------------------------------------------------- |
| **`for`**       | Sequential      | Consumptive or Cloning  | Sequential data processing with temporal pacing.    |
| **`split_map`** | Parallel        | Isolated Snapshots      | Computation-intensive parallel mapping operations.   |
| **`loop`**      | Repeated        | Entropic state rules    | Periodic tasks with deterministic temporal budgets. |
| **`loop tick`** | Isochronous     | Phase-committed commits | Real-time control systems and isochronous pipelines. |
