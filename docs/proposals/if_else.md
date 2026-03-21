# ICTL Proposal: Deterministic Branching & Causal Loops

This document outlines the formal requirements for control flow within the ICTL Virtual Machine, ensuring that branching logic does not introduce temporal jitter or entropic ambiguity.

## 1. Temporal Equalization (The "Padding" Rule)
In ICTL, time is a strict resource. Standard branching (if/else) typically introduces "jitter" because different code paths take different amounts of time. ICTL eliminates this via **Temporal Equalization**.

* **The Feature**: The execution time of an `if/else` block is always equal to the **Worst-Case Execution Time (WCET)** of all possible paths.
* **The Logic**: If the `if` branch executes in $t_{if}$ and the `else` branch in $t_{else}$, the total cost $T$ is:
    $$T = \max(t_{if}, t_{else}) + t_{overhead}$$
* **Implementation**: The VM calculates the instruction costs before entry. Upon exiting the shorter branch, the VM injects "Temporal No-Ops" (stalling the `local_clock`) until parity is reached. This ensures the `global_clock` advances identically regardless of the logical path.



---

## 2. Enriched Entropic Reconciliation
When a variable enters a "Quantum State" (modified in one branch but not another), it must be unified upon branch exit to maintain entropic stability.

* **The Feature**: A `reconcile` block suffix is mandatory for any variable whose entropic state differs across branches.
* **Semantics**:
    * **`first_wins`**: Favors the state produced by the physically first branch in the source code.
    * **`priority(branch_name)`**: Explicitly favors the state of a specific named path (e.g., favors the `if` over the `else`).
    * **`decay`**: A conservative fallback; if states differ, the variable is marked as `Decayed` and becomes unusable.
* **Strict Restraint**: The Analyzer will fail if a variable is "Entropy-Ambiguous" at the exit point of a block without a reconciliation rule.

---

## 3. Bounded "Fuel" Loops & Explicit Break
Infinite loops are prohibited to prevent "Temporal Black Holes" where a branch consumes infinite global time.

* **Temporal Fuel**: Every loop must declare a maximum "Fuel" limit in milliseconds (`ms`). 
* **Loop Body Stats**: The Analyzer calculates the **Entropic Cost** per iteration.
* **Explicit Break**: The `break` keyword is a first-class temporal exit. When a `break` is triggered, the VM must still account for the "Remaining Fuel" to maintain deterministic timing, or the loop must be wrapped in a `Watchdog` for asynchronous termination.

```ictl
// Syntax Example
loop (max 50ms) {
    let task = chan_recv(pipe)
    if (task == "stop") {
        break
    }
    // Static Analyzer knows each iteration costs ~2ms
}
```

---

## The "Perfect" ICTL Branch Comparison

| Feature | Standard `if` | ICTL "Perfect" `if` |
| :--- | :--- | :--- |
| **Timing** | Variable (Jitter) | **Deterministic (Padded)** |
| **Variable State** | Ambiguous | **Unified via Reconciliation** |
| **Cost** | Free | **1ms check fee + $\max(paths)$** |
| **Loops** | Unbounded | **Fuel-Limited (Bounded Entropy)** |

### Comprehensive Syntax Example

```ictl
@0ms: {
    let data = "original"
    let check = true
    
    // The entire block is locked to 6ms (5ms Network + 1ms overhead)
    if (check) {
        let result = NetworkRequest(domain="ictl.org") // 5ms
        let _ = data // consume data in this branch by expression read
    } else {
        let result = "local_cache" // 1ms
        // VM adds 4ms padding here automatically
    } 
    reconcile {
        data: decay,        // data is now unusable because it was only consumed in one path
        result: first_wins  // result takes the value from the 'if' path if successful
    }
}
```

---

### Why this works for ICTL
This isn't just about safety—it's about **Performance Predictability**. In 2026, where we are coordinating thousands of parallel timelines, we cannot afford for a single "else" block to offset the synchronization of an entire system.