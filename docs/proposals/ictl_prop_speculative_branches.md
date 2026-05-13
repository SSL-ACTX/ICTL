# Proposal: ICTL Speculative Execution Branches

This document specifies the formal requirements for transactional execution within the Isolate Concurrent Temporal Language (ICTL) Virtual Machine. It introduces **Speculative Control Flow**, facilitating operations that may incur failure or exceed temporal constraints to execute securely without compromising the parent timeline's entropic state.

## 1. Limitations of Conventional Exception Handling
If an operation within ICTL consumes a variable but fails prior to the conclusion of its logic, the variable is permanently lost, resulting in cascading causal failures. ICTL requires "Zero Entropic Leakage" upon execution failure.

## 2. The `speculate` Mechanism
The `speculate` construct initializes a **Micro-Timeline**, utilizing a snapshot of the current memory arena. 
* Upon successful execution, the state is integrated into the parent arena via the `commit` primitive.
* In the event of an error, temporal budget exhaustion, or the execution of the `collapse` keyword, the micro-timeline is terminated. The parent's memory arena remains unmodified, representing a localized acausal reset.

### Formal Syntax Specification

```ictl
@0ms: {
  let critical_data = [1, 2, 3]

  speculate (max 50ms) {
    // Variable consumption within the micro-timeline
    let temp = critical_data
    let result = temp 
    
    if (result == "Timeout") {
      collapse // Manual termination. 'critical_data' is restored in the parent context.
    }
    
    // Formal integration of state into the parent arena
    commit {
      let final_output = result
    }

  } fallback {
    // Executes only upon speculation failure (collapse or timeout).
    // 'critical_data' maintains validity within this scope.
    let final_output = "System_Default"
  }
}
```

## 3. Semantics and Entropic Regulations

### Micro-Isolation Invariants
- Entry into a `speculate` block triggers a snapshot of the current memory arena (equivalent to an implicit `anchor`).
- Any `consume` or `decay` operations within the block are restricted to the micro-timeline.

### Speculation Termination Protocols
- **`commit { ... }`**: Successfully concludes the speculation. Statements within the `commit` block are integrated into the parent arena; other speculative variables are discarded. The parent's entropic state is updated accordingly.
- **`collapse`**: Immediately terminates block execution and discards internal state. The parent arena is restored to its precise pre-speculation configuration.
- **Temporal Timeout**: If the micro-timeline's `local_clock` exceeds the `(max Xms)` budget, the VM initiates an automated `collapse`.

### Deterministic Temporal Padding (The Padding Rule)
To maintain the ICTL principle of deterministic temporal execution, the `speculate / fallback` construct must maintain invariant temporal cost, irrespective of whether the execution path succeeds or fails.

* **Temporal Cost Equation**: The total duration $T$ is the sum of the maximum speculative duration and the Worst-Case Execution Time (WCET) of the fallback:
  $$T = T_{max\_duration} + T_{fallback\_wcet}$$
* **Implementation Logic**: The Stack-based Temporal Virtual Machine (STVM) automatically pads the `global_clock` upon exiting the construct to ensure absolute predictability.

## 4. Technical Implementation: Structured State Decay

```ictl
@0ms: {
  let user = struct { id=1, name="Alice", token="abc" }

  speculate (max 10ms) {
    // Structural decay restricted to the micro-timeline arena
    let t = user.token 
    let valid = validate_authentication(t)
    
    if (valid == false) {
      collapse
    }
    
    commit (is_authenticated = valid)
  } fallback {
    // Upon authentication failure, the parent structure maintains integrity.
    chan_send retry_interface(user) 
    let is_authenticated = false
  }
}
```

## 5. Runtime Operational Modes

To facilitate diverse commit semantics, the STVM supports the following configuration options:

- `SpeculationCommitMode::Selective` (Default): Only variables explicitly designated within the `commit` block are integrated into the parent timeline.
- `SpeculationCommitMode::Full`: The comprehensive state of the successful speculative timeline is integrated into the parent.

This architecture enables compatibility with both strict contract-based commits and aggressive micro-timeline promotion during speculative execution.
