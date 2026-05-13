# Speculative Execution Branches

This document formalizes the syntax and execution semantics of the `speculate`, `fallback`, and `collapse` constructs within the Isolate Concurrent Temporal Language (ICTL).

---

## 1. Foundational Principles

The `speculate` construct implements a transient micro-timeline designed for trial computations with guaranteed zero-impact state restoration.

### Architectural Objectives
- **Zero Leakage Invariant**: No side effects or entropic modifications affect the parent timeline unless a formal `commit` operation is successfully executed.
- **State Reversion Transparency**: The Stack-based Temporal Virtual Machine (STVM) can revert all memory arena modifications initiated within a speculative block in O(1) temporal complexity.
- **Temporal Accountability**: Speculative execution occupies a deterministic duration, irrespective of whether the trial is committed or reverted.

---

## 2. Formal Syntax

```ictl
speculate (max <amount>ms) {
    <statements>
    commit { <statements> }
} [fallback { <statements> }]
```

### Functional Components
- **Temporal Budget (`max <amount>ms`)**: Defines the maximum duration permitted for the speculative block. Exceeding this limit triggers an automated timeout, equivalent to a `collapse` event.
- **Commit Block (`commit { ... }`)**: Designates a successful trial. Statements within this block define the final state to be committed to the parent timeline.
- **Collapse Primitive (`collapse`)**: Immediately terminates the active speculative block and initiates the `fallback` logic.
- **Fallback Block (`fallback { ... }`)**: Executes logic only when a speculation terminates unsuccessfully due to `collapse` or temporal timeout.

---

## 3. Commit Protocols

The `speculation_mode` directive specifies the mechanism for merging a successful trial with the parent memory arena.

| Protocol        | Execution Behavior                                                                                                                                     |
| :-------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`selective`** | **Default Configuration**. Only variables explicitly modified or assigned within the `commit { ... }` block are integrated into the parent timeline. |
| **`full`**      | The entire speculative micro-timeline arena (including variables modified prior to the `commit` block) is integrated into the parent timeline.        |

### Directive Syntax
```ictl
speculation_mode(selective | full)
```

---

## 4. Temporal Cost and Padding Mechanisms

The total temporal cost of a `speculate` block is invariant, ensuring deterministic execution behavior.

**Temporal Cost Equation:**
```
total_cost = 1ms (overhead) + max_ms + fallback_wcet
```
The STVM consistently pads the `local_clock` to satisfy this total cost, ensuring that the parent timeline's clock remains identical regardless of the success or failure of the speculation.

---

## 5. Causal Anchor Constraints

Successful `commit` operations establish a formal **commit horizon**. Anchors initialized prior to a `commit` event become inaccessible via `rewind_to`, as the entropic states of the variables undergo a foundational transformation during the commit process.
