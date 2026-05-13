# Advanced Temporal Routing Mechanisms

This document specifies the advanced control-flow constructs within the Isolate Concurrent Temporal Language (ICTL), specifically the `select` and `match entropy` primitives.

---

## 1. Temporal Multiplexing (`select`)

The `select` construct is utilized to facilitate a temporal race between multiple channel operations and a designated timeout period.

### Formal Syntax
```ictl
select (max <amount>ms) {
    case <identifier> = chan_recv(<chan>):
        <statements>
    timeout:
        <statements>
} [reconcile (<resolution_rules>)]
```

### Execution Semantics
- **Bounded Wait Protocol**: The `select` primitive restricts the execution duration of its constituent cases to the specified `max <amount>ms`.
- **Case Prioritization**: The initial `case` whose associated channel achieves readiness (contains data) is selected for execution.
- **Timeout Execution**: If no channel operations achieve readiness within the specified temporal window, the `timeout:` block is executed.
- **Clock Uniformity Invariant**: The `local_clock` is advanced by `1ms + max(case_wcet, timeout_wcet)` through deterministic padding. This ensures that the statement's temporal cost remains invariant regardless of the execution path.

---

## 2. Entropic State Routing (`match entropy`)

The `match entropy` construct facilitates control flow branching based on the current entropic state of a variable, ensuring semantic safety.

### Formal Syntax
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

### Classification of Entropic States
- **`Valid`**: The variable remains intact and is fully owned within the current arena.
- **`Pending`**: The variable represents an unresolved temporal promise (e.g., resulting from a `defer` operation).
- **`Decayed`**: The variable is a structured type where one or more constituent fields have been consumed.
- **`Consumed`**: The variable has undergone movement or destructive read operations.

**Implementation Note**: Within the `Valid`, `Pending`, and `Decayed` branches, a localized binding is established for the variable's payload, facilitating its utilization within the block scope.

---

## 3. Comparative Analysis of Control Flow Primitives

| Primitive           | Functional Objective | Technical Advantage                       |
| :------------------ | :----------------- | :---------------------------------------- |
| **`if`**            | Boolean Predicate. | Explicit entropic state reconciliation.   |
| **`select`**        | Temporal Race.     | Deterministic timing for channel I/O.     |
| **`match entropy`** | State Verification. | Safe management of consumed/decayed values.|
