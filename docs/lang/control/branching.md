# Branching Control Flow

This document specifies the behavior of conditional branches and structural pattern matching in ICTL, including speculative execution and entropic state reconciliation.

---

## 1. Conditional Execution (`if` / `else`)

The `if` statement allows for runtime selection between two execution paths based on a boolean condition.

### Syntax
```ictl
if (<condition>) <statement_block> [else <statement_block>] [reconcile (<resolution_rules>)]
```

### Semantics
- **Truthiness**: Values are truthy if they are non-zero integers or non-empty strings.
- **Deterministic Padding**: If the target clock is less than the calculated maximum cost across all possible paths, the VM performs deterministic padding to ensure time-invariance.
- **Static Analysis**: The static analyzer evaluates both the `if` and `else` blocks to ensure entropic consistency.
- **Reconciliation**: Any value consumed in one path but not the other results in an error unless a `reconcile` rule is provided.

### State Reconciliation Strategies

| Strategy             | Description                                                                                   |
| :------------------- | :-------------------------------------------------------------------------------------------- |
| **`first_wins`**     | Uses the value from the branch that executed if available, otherwise falls back to the other. |
| **`priority(if)`**   | Always prefers the value produced in the `then` branch if a conflict occurs.                  |
| **`priority(else)`** | Always prefers the value produced in the `else` branch.                                       |
| **`decay`**          | Marks the variable as `Consumed` or `Decayed` regardless of the executed path.                |

---

## 2. Structural Pattern Matching (`match entropy`)

The `match entropy` statement provides a way to branch based on the **entropic state** of a variable or expression. This is primarily used to handle partial data structures (decayed structs) or pending promises.

### Syntax
```ictl
match entropy(<expression>) {
    Valid(<binding>):  <statement_block>
    Decayed(<binding>): <statement_block>
    Pending:           <statement_block>
    Consumed:          <statement_block>
}
```

### Arm Semantics
- **`Valid(v)`**: Executed if the target is completely whole. `v` binds to the full payload.
- **`Decayed(d)`**: Executed if one or more fields of a struct/topology have been consumed. `d` binds to the remaining fragments.
- **`Pending`**: Executed if the target represents an unresolved promise (e.g., a network request).
- **`Consumed`**: Executed if the target has already been destructively read or sent.

### Example: Handling Partial State
```ictl
@0ms: {
    let p = { a = 1, b = 2 }
    let val = p.a  // 'p' is now Decayed
    
    match entropy(p) {
        Valid(full): 
            print("Whole")
        Decayed(partial):
            print("Partial data: " + partial.b)
        Consumed:
            print("Gone")
    }
}
```

---

## 3. Advanced Branching (`select`)

The `select` statement branches based on which asynchronous event completes first, typically used with channels or promises.

### Syntax
```ictl
select (max <amount>ms) {
    case <binding> = <source>: <statement_block>
    ...
} [timeout { <statements> }]
```

### Semantics
- **Causal Racing**: The first branch to have its `source` become ready (e.g., a message arrived in a channel) wins.
- **Deterministic Window**: If no branch completes within `max <amount>ms`, the `timeout` block is executed.
- **Cost**: The cost is always `max + overhead`, ensuring deterministic timing regardless of which event won the race.

---

## 4. Speculative Rollback and Causal Safety

The `speculate` block provides a sandbox for potentially failing operations.

- **Isolation**: Changes within a `speculate` block are only committed if the block completes successfully and reaches a `commit` point.
- **Surgical Rollback**: If a speculation fails or collapses, the VM performs a surgical rollback of all `Arena` changes and any causal effects (like channel operations) that occurred inside the block.
- **Causal Consistency**: Similar to `rewind_to`, a speculation cannot be rolled back if it has already influenced an external timeline (e.g., a message sent during speculation was consumed by another branch). This prevents timeline corruption.
