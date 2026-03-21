# ICTL Speculative Branches (`speculate` / `fallback`)

This document outlines the formal requirements for safe, transactional execution within the ICTL Virtual Machine. It introduces **Speculative Control Flow**, allowing operations that might fail or exceed temporal bounds to execute safely without permanently corrupting the parent timeline's entropic state.

## 1. The Problem with Traditional `try/catch`
If an operation in ICTL consumes a variable but fails before completing its logic, the variable is permanently lost, causing cascading causal failures. ICTL requires "Zero Entropic Leakage" upon failure.

## 2. The `speculate` Mechanic
Instead of a simple block, `speculate` forks a **Micro-Timeline**. It borrows the current arena's state. 
* If the block succeeds, the programmer explicitly merges the new state back using `commit`.
* If the block encounters an error, exceeds its time budget, or hits the explicit `collapse` keyword, the micro-timeline is instantly destroyed. The parent's memory arena remains completely untouched, as if the attempt never happened (a localized acausal reset).

### Syntax Overview

```ictl
@0ms: {
  let critical_data = [1, 2, 3]

  speculate (max 50ms) {
    // Consume data in this micro-timeline via assignment (identifier read is destructive)
    let temp = critical_data
    let result = temp // placeholder for computation
    
    if (result == "Timeout") {
      collapse // Manually abort. 'critical_data' is fully restored in the parent.
    }
    
    // Explicitly push state back to the parent arena
    commit {
      let final_output = result
    }

  } fallback {
    // Runs ONLY if the speculate block collapses or times out.
    // 'critical_data' is still valid here.
    let final_output = "Offline Default"
  }
}
```

## 3. Semantics & Entropic Rules

### Micro-Isolation
- Upon entering `speculate`, the VM snapshots the current memory arena (similar to an implicit `anchor`).
- Any `consume` or `decay` operations inside the block apply *only* to the micro-timeline.

### Exiting the Speculation
- **`commit { ... }`**: Successfully closes the speculation. Statements inside `commit` are applied to the parent arena, while other speculative child variables are dropped. The parent's original entropic state is updated with committed values.
- **`collapse`**: Immediately halts execution of the block. All internal state is discarded. The parent arena is perfectly restored to the exact state it was in before `speculate` began.
- **Timeout**: If the `local_clock` of the micro-timeline exceeds the `(max Xms)` budget, the VM forces an automatic `collapse`.

### Temporal Equalization (The Padding Rule)
To maintain the core ICTL philosophy of deterministic time, a `speculate / fallback` construct must always cost exactly the same amount of time, regardless of whether it succeeds in 2ms or fails at 49ms.

* **The Formula**: The total time $T$ taken by the entire construct is strictly bound to the maximum allowed speculation time plus the fallback's worst-case execution time (WCET):
  $$T = T_{max\_fuel} + T_{fallback\_wcet}$$
* **Implementation**: The VM will automatically pad the `global_clock` with Temporal No-Ops upon exiting the construct to ensure absolute predictability.

## 4. Example: Safe Structural Decay

```ictl
@0ms: {
  let user = struct { id=1, name="Alice", token="abc" }

  speculate (max 10ms) {
    // Decays the 'user' struct inside the micro-timeline
    let t = user.token 
    let valid = validate_auth(t)
    
    if (valid == false) {
      collapse
    }
    
    commit (is_authed = valid)
  } fallback {
    // If auth fails, the struct was never decayed in reality.
    // We can still send the whole struct somewhere else.
    chan_send retry_pipe(user) 
    let is_authed = false
  }
}
```

## 5. Runtime Behavior Modes (New)

To support varying commit semantics, the VM now has a runtime configuration option:

- `SpeculationCommitMode::Selective` (default): only `commit`-visible variables get injected into the parent timeline.
- `SpeculationCommitMode::Full`: all variables from the successful speculative timeline are merged into the parent state.

Use:

```rust
vm.set_speculative_commit_mode(ictl::runtime::vm::SpeculationCommitMode::Full);
```

This choice enables compatibility with both strict contract-style commit and more aggressive micro-timeline promotion during speculation. 


---

### Why this fits ICTL perfectly:
This avoids the messy scope-leaks of `try/catch`. It perfectly leverages the `anchor` and `reset` mechanics you built in Phase 13, but wraps them in a structured, developer-friendly control flow. It treats an error not as an exception, but as a **Timeline Pruning Event**.
