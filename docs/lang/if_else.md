# Conditional Execution (`if` / `else`)

This document specifies the behavior of conditional branches in ICTL, including speculative execution and state reconciliation.

---

## 1. Syntax

```ictl
if (<condition>) <statement_block> [else <statement_block>] [reconcile (<resolution_rules>)]
```

**Conditionals:**
- Values are truthy if they are non-zero integers or non-empty strings.
- Expressions are evaluated in the branch's current context.

---

## 2. Semantics

### Runtime Execution
- Only one path is executed at runtime based on the boolean result of the condition.
- The `local_clock` is advanced by the cost of the executed path plus a 1ms overhead for the branch itself.
- If the target clock is less than the calculated maximum cost across all possible paths, the VM performs **deterministic padding**.

### Static Analysis
- The static analyzer evaluates both the `if` and `else` blocks simultaneously to ensure entropic consistency.
- Any value consumed in one path but not the other results in a `CrossPathConsume` error unless a `reconcile` rule is provided.

---

## 3. State Reconciliation (`reconcile`)

When a variable is modified or consumed asynchronously across different branches of the same `if` statement, reconciliation rules determine the final state of that variable in the parent's arena.

| Strategy             | Description                                                                                   |
| :------------------- | :-------------------------------------------------------------------------------------------- |
| **`first_wins`**     | Uses the value from the branch that executed if available, otherwise falls back to the other. |
| **`priority(if)`**   | Always prefers the value produced in the `then` branch if a conflict occurs.                  |
| **`priority(else)`** | Always prefers the value produced in the `else` branch.                                       |
| **`decay`**          | Marks the variable as `Consumed` or `Decayed` regardless of the executed path.                |

---

## 4. Examples

### Basic Reconcile
```ictl
@0ms: {
  let x = "source"
  if (flag == 1) {
    let y = x
    // x is now consumed in this path
  } else {
    let z = "static"
    // x is still valid in this path
  } reconcile (x=first_wins)
}
```

### Resolving Variable Conflicts
```ictl
@0ms: {
  if (mode == "fast") {
    let timeout = 10ms
  } else {
    let timeout = 100ms
  } reconcile (timeout=priority(if))
}
```
In this example, `timeout` will be 10ms if `mode == "fast"`, otherwise 100ms.
