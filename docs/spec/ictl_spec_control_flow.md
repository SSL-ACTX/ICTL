# Branching Control Flow Mechanisms

This document specifies the formal behavior of conditional branching and structural pattern matching within the Isolate Concurrent Temporal Language (ICTL), including the protocols for speculative execution and entropic state reconciliation.

---

## 1. Conditional Execution Primitives (`if` / `else`)

The `if` statement facilitates the runtime selection between divergent execution paths based on a boolean predicate.

### Formal Syntax
```ictl
if (<condition>) <statement_block> [else <statement_block>] [reconcile (<resolution_rules>)]
```

### Execution Semantics
- **Predicate Evaluation**: Values are evaluated as truthy if they represent non-zero integers or non-empty string literals.
- **Deterministic Temporal Padding**: If the execution duration is less than the calculated maximum cost across all possible paths, the Stack-based Temporal Virtual Machine (STVM) performs deterministic padding to maintain time-invariance.
- **Static Semantic Analysis**: The analyzer evaluates both the `if` and `else` blocks to ensure entropic consistency across all execution branches.
- **State Reconciliation**: Variables consumed in one path but not the other result in a semantic error unless a formal `reconcile` protocol is specified.

### State Reconciliation Protocols

| Protocol Identifier  | Functional Description                                                                                        |
| :------------------- | :------------------------------------------------------------------------------------------------------------ |
| **`first_wins`**     | Prioritizes the value from the executed branch if available; otherwise, utilizes the value from the alternate. |
| **`priority(if)`**   | Always prioritizes the value produced within the `then` branch in the event of a conflict.                    |
| **`priority(else)`** | Always prioritizes the value produced within the `else` branch.                                               |
| **`decay`**          | Transitions the variable to the `Consumed` or `Decayed` state regardless of the executed path.                |

---

## 2. Structural Pattern Matching (`match entropy`)

The `match entropy` construct facilitates control flow branching based on the **entropic state** of a variable or expression. This mechanism is primarily utilized to handle partial data structures (decayed structures) or unresolved temporal promises.

### Formal Syntax
```ictl
match entropy(<expression>) {
    Valid(<binding>):  <statement_block>
    Decayed(<binding>): <statement_block>
    Pending:           <statement_block>
    Consumed:          <statement_block>
}
```

### Arm Semantics
- **`Valid(v)`**: Executed if the target is in a pristine state. `v` binds to the comprehensive payload.
- **`Decayed(d)`**: Executed if one or more fields of a structure or topology have been consumed. `d` binds to the residual fragments.
- **`Pending`**: Executed if the target represents an unresolved promise (e.g., a deferred network operation).
- **`Consumed`**: Executed if the target has undergone destructive read or transmission.

### Technical Implementation: Partial State Management
```ictl
@0ms: {
    let p = { a = 1, b = 2 }
    let val = p.a  // 'p' transitions to the Decayed state
    
    match entropy(p) {
        Valid(full): 
            System.Log(message="Comprehensive state maintained")
        Decayed(partial):
            System.Log(message="Partial state detected: " + partial.b)
        Consumed:
            System.Log(message="State terminal")
    }
}
```

---

## 3. Asynchronous Branching (`select`)

The `select` primitive facilitates branching based on the temporal priority of asynchronous events, typically involving communication channels or promises.

### Formal Syntax
```ictl
select (max <amount>ms) {
    case <binding> = <source>: <statement_block>
    ...
} [timeout { <statements> }]
```

### Execution Semantics
- **Causal Competition**: The first branch whose `source` becomes ready (e.g., arrival of a channel message) is selected for execution.
- **Deterministic Temporal Window**: If no branch achieves readiness within the `max <amount>ms` interval, the `timeout` block is executed.
- **Temporal Cost**: The execution cost is invariant at `max + overhead`, ensuring deterministic timing regardless of the winning branch.

---

## 4. Speculative Rollback and Causal Integrity

The `speculate` block provides an isolated environment for execution paths that may require reversion.

- **Isolation Invariants**: Modifications within a `speculate` block are committed only upon reaching a formal `commit` point.
- **Surgical State Reversion**: Upon failure or explicit `collapse`, the STVM performs a surgical rollback of all memory arena modifications and causal effects (e.g., channel operations) initiated within the block.
- **Causal Consistency Enforcement**: A speculative execution cannot be reverted if it has influenced an external timeline (e.g., transmission of a message consumed by another branch). This prevents systemic timeline corruption.
