# Proposal: ICTL Deterministic Branching and Causal Loops

This document specifies the formal requirements for control flow within the Isolate Concurrent Temporal Language (ICTL) Virtual Machine, ensuring that branching logic maintains temporal invariance and entropic stability.

## 1. Deterministic Temporal Equalization (The Padding Protocol)
Within the ICTL framework, temporal resources are strictly governed. Conventional branching mechanisms (if/else) typically introduce temporal jitter, as divergent execution paths may possess distinct durations. ICTL eliminates this variability through **Temporal Equalization**.

* **Architectural Requirement**: The execution duration of an `if/else` block is invariant and equal to the **Worst-Case Execution Time (WCET)** of all possible execution paths.
* **Execution Logic**: Given an `if` path with duration $t_{if}$ and an `else` path with duration $t_{else}$, the total temporal cost $T$ is defined as:
    $$T = \max(t_{if}, t_{else}) + t_{overhead}$$
* **Technical Implementation**: The analyzer calculates instruction costs prior to block entry. Upon completion of the shorter execution path, the Stack-based Temporal Virtual Machine (STVM) injects "Temporal No-Ops" to stall the `local_clock` until parity is achieved. This ensures that the `global_clock` advances identically regardless of the selected logical path.

---

## 2. Formal Entropic Reconciliation
When a variable's state diverges across branches—entering what is conceptualized as a "Quantum State"—it must undergo formal unification upon block exit to maintain systemic entropic stability.

* **Architectural Requirement**: A `reconcile` block suffix is mandatory for any variable whose entropic state differs across execution branches.
* **Resolution Protocols**:
    * **`first_wins`**: Prioritizes the state produced by the branch that appears first within the source code.
    * **`priority(branch_identifier)`**: Explicitly prioritizes the state of a designated execution path (e.g., favoring the `if` path over the `else`).
    * **`decay`**: A conservative resolution mechanism where state divergence results in the variable being transitioned to the `Decayed` state, rendering it inaccessible.
* **Semantic Constraint**: The analyzer triggers a failure if a variable occupies an "Entropy-Ambiguous" state at the block exit point in the absence of a formal reconciliation rule.

---

## 3. Bounded Temporal Fuel Loops and Explicit Termination
Infinite iterative execution is prohibited to prevent "Temporal Black Holes," where an execution branch consumes infinite global time.

* **Temporal Fuel Allocation**: Every iterative construct must specify a maximum "Fuel" limit, measured in milliseconds (`ms`). 
* **Entropic Cost Analysis**: The analyzer calculates the entropic cost per iteration during the compilation phase.
* **Explicit Termination (`break`)**: The `break` keyword facilitates a formal temporal exit. Upon a `break` event, the STVM must account for the "Remaining Fuel" to maintain deterministic timing, or the loop must be encapsulated within a `watchdog` primitive for asynchronous termination.

```ictl
// Technical Implementation Example
loop (max 50ms) {
    let task = chan_recv(data_pipe)
    if (task == "terminate") {
        break
    }
    // Static Analysis identifies each iteration cost as ~2ms
}
```

---

## Comparative Analysis of Branching Mechanisms

| Feature | Conventional `if` | ICTL Deterministic `if` |
| :--- | :--- | :--- |
| **Temporal Consistency** | Variable (Jitter) | **Deterministic (Padded)** |
| **State Stability** | Ambiguous | **Unified via Reconciliation** |
| **Execution Cost** | Opaque | **Fixed (1ms overhead + max(paths))** |
| **Iterative Bounds** | Unbounded | **Fuel-Limited (Bounded Entropy)** |

---

### Architectural Significance
This architecture ensures systemic **Performance Predictability**. In high-concurrency environments coordinating thousands of parallel timelines, temporal synchronization must be maintained with absolute precision, preventing divergent logical paths from compromising systemic integrity.
