# Advanced Routing constructs

This document details the advanced control-flow constructs in ICTL, including `select` and `match entropy`.

---

## 1. Temporal Multiplexing (`select`)

The `select` construct is used to race multiple channel operations and a timeout period.

### Syntax
```ictl
select (max <amount>ms) {
    case <identifier> = chan_recv(<chan>):
        <statements>
    timeout:
        <statements>
} [reconcile (<resolution_rules>)]
```

### Semantics
- **Bounded Wait**: `select` limits the execution time of its cases to `max <amount>ms`.
- **Case Execution**: The first `case` whose channel has data is executed.
- **Timeout**: If no channel has data before the timeout, the `timeout:` block is executed.
- **Clock Uniformity**: The `local_clock` is advanced by `1ms + max(case_wcet, timeout_wcet)` with deterministic padding, ensuring the statement always costs the same regardless of which path was taken.

---

## 2. State-Based Routing (`match entropy`)

Use `match entropy` to safely branch based on the current entropic state of a variable.

### Syntax
```ictl
match entropy(<identifier>) {
    Valid(<binding>):
        <statements>
    Pending(<binding>):
        <statements>
    Decayed(<binding>):
        <statements>
    Consumed:
        <statements>
}
```

### Branched States
- **`Valid`**: The variable is intact and fully owned.
- **`Pending`**: The variable is an unresolved promise (e.g., from `defer`).
- **`Decayed`**: The variable is a struct with one or more consumed fields.
- **`Consumed`**: The variable has been moved or destructively read.

**Note**: In `Valid`, `Pending`, and `Decayed` branches, a new local binding is introduced for the matched variable's payload, allowing its use within the block.

---

## 3. Comparison with Standard Flow

| Construct           | Purpose            | Advantage                                 |
| :------------------ | :----------------- | :---------------------------------------- |
| **`if`**            | Boolean condition. | Explicit state reconciliation.            |
| **`select`**        | Temporal race.     | Deterministic timing for channel IO.      |
| **`match entropy`** | State inspection.  | Safe handling of consumed/decayed values. |
