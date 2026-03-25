# Paced Iteration and Parallelism (Loops)

This document specifies the iterative constructs in ICTL and their deterministic temporal and entropic semantics.

---

## 1. Paced `for` loop

The `for` loop in ICTL is designed for sequential iteration with fixed-time execution for each item.

### Syntax
```ictl
for <item> <mode> <source> [pacing <amount>ms] [(max <amount>ms)] {
  <statements>
}
```

**Modes:**
- **`consume`**: The `<source>` collection is destructively read from the arena. Each `<item>` is moved into the loop scope.
- **`clone`**: The `<source>` remains intact. Each `<item>` is a deep copy of the original element (at temporal cost).

**Pacing (`pacing <N>ms`):**
- Ensures each iteration of the loop body takes exactly `Nms`.
- If the body executes in less than `Nms`, the `local_clock` is **padded**.
- If the body exceeds `Nms`, a `WatchingBite` is triggered.

**Budget (`max <N>ms`):**
- Defines the total time boundary for the entire loop's execution. If the loop completes before the `max` budget, the remaining time is added to the `local_clock`.

---

## 2. Fixed-Frequency `loop`

The `loop` statement provides a way to execute a block of code repeatedly with a deterministic period.

### Syntax
```ictl
loop (max <amount>ms) {
  <statements>
  [break]
}
```

### `loop tick` (Real-Time Control)
The `loop tick` is a specialized form of the loop intended for isochronous tasks.

```ictl
loop tick {
  <statements>
  [break]
}
```

- **Isochronous Execution**: Each `tick` is synchronized to a fixed time slice (e.g., `1ms` or `10ms`) defined by the current `slice` context.
- **Phase Commitment**: State changes are committed at the end of each tick boundary.

---

## 3. Scatter-Gather Parallelism (`split_map`)

The `split_map` construct implements a deterministic parallel mapping pattern.

### Syntax
```ictl
split_map <item> <mode> <source> {
  <statements>
  [yield <expression>]
} [reconcile (<resolution_rules>)]
```

**Semantics:**
1. **Parallel Spawning**: One child timeline is spawned for each element in the `<source>` collection.
2. **Snapshot Execution**: Each child starts with a snapshot of the parent state.
3. **Isolation**: Child timelines execute the body independently.
4. **Data Aggregation**: Values emitted via `yield` in each child are collected into a hidden `splitmap_results` array back in the parent timeline.

### Reconcile
Conflicts resulting from multiple children modifying the same shared variables (cloned from the parent) are resolved via `reconcile` rules.

---

## 4. Comparison of Iterative Constructs

| Construct       | Execution Model | Memory Impact           | Best For                                            |
| :-------------- | :-------------- | :---------------------- | :-------------------------------------------------- |
| **`for`**       | Sequential      | Consumes or Clones      | Sequential data processing.                         |
| **`split_map`** | Parallel        | Isolated clones         | Compute-intensive parallel map/reduce.              |
| **`loop`**      | Repeated        | Standard entropic rules | Fixed-time repeated tasks (like system monitoring). |
| **`loop tick`** | Isochronous     | Phase-committed         | Real-time control loops and pipelines.              |
