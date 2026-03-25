# Speculative Branches

This document formalizes the syntax and semantics of the `speculate`, `fallback`, and `collapse` constructs in ICTL.

---

## 1. Core Principles

The `speculate` construct implements a transient micro-timeline for trial computations with zero-impact rollback.

### Design Goals
- **Zero Leakage**: No side effects or entropic modifications reach the parent timeline unless a successful `commit` is executed.
- **Rollback Transparency**: The VM can "undo" all arena changes made within a speculative block in O(1) time.
- **Temporal Accountability**: The speculatively executed code costs a deterministic amount of time, regardless of whether it was committed or rolled back.

---

## 2. Syntax

```ictl
speculate (max <amount>ms) {
    <statements>
    commit { <statements> }
} [fallback { <statements> }]
```

### Components
- **`max <amount>ms`**: The temporal budget. Overrunning this time limit triggers a timeout (equivalent to `collapse`).
- **`commit { ... }`**: Marks a successful trial. All statements within this block modify the final committed state.
- **`collapse`**: Immediately aborts the current speculative block and triggers the `fallback`.
- **`fallback { ... }`**: Statements executed only when a speculation fails due to `collapse` or a timeout.

---

## 3. Commit Modes

The `speculation_mode` statement determines how a successful `commit` merges with the parent's arena.

| Mode            | Behavior                                                                                                                                              |
| :-------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`selective`** | **Default**. Only variables explicitly assigned or modified within the `commit { ... }` block are merged back into the parent.                        |
| **`full`**      | The entire micro-timeline arena (including variables modified outside the `commit` but still within the `speculate` block) is merged into the parent. |

### Syntax
```ictl
speculation_mode(selective | full)
```

---

## 4. Operational Cost and Padding

The total temporal cost of a `speculate` block is fixed to ensure deterministic behavior.

**Cost Formula:**
```
total_cost = 1ms (overhead) + max_ms + fallback_wcet
```
The VM always pads the `local_clock` to this total cost, ensuring that whether a speculation succeeds or fails, the resulting parent clock remains identical.

---

## 5. Causal Anchors

Successful `commit` calls establish a **commit horizon**. Anchors established before a `commit` cannot be reached via `rewind_to`, as the entropic states of the variables have been fundamentally altered by the commit process.
