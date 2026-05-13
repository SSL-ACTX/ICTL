# Proposal: ICTL Advanced Routing (Select and Entropic Match)

This document specifies the formal requirements for advanced routing control flow within the Isolate Concurrent Temporal Language (ICTL). It introduces **Temporal Multiplexing** for communication channels and **State-Based Routing** for entropic memory management.

## 1. Relativistic Temporal Multiplexing (`select`)

With the implementation of Entropic Channels, execution timelines must frequently monitor multiple data streams concurrently. The ICTL `select` primitive facilitates this through a mechanism strictly bounded by the `global_clock` and governed by the Deterministic Padding Rule.

### Operational Mechanics
- **Temporal Boundary**: A `select` block must specify a `max` duration for resolution. 
- **Deterministic Padding Protocol**: Regardless of which channel resolves or if a timeout occurs, the total temporal cost of the `select` block remains invariant, fixed to the `max` duration plus the Worst-Case Execution Time (WCET) of the selected branch.
- **Destructive Read**: The case that successfully retrieves data destructively consumes it from the source channel. Unselected channels maintain their prior state.

### Syntax Specification

```ictl
@0ms: {
  // Monitor telemetry and command channels for data resolution
  select (max 100ms) {
    case data = chan_recv(telemetry_pipe):
      let processed = call parse_telemetry(data) // WCET: 10ms
      
    case cmd = chan_recv(command_pipe):
      let executed = call run_command(cmd)       // WCET: 25ms
      
    timeout:
      // Executed if resolution does not occur within the 100ms window
      System.Log(message="No channel resolution within temporal window.")
      
  } reconcile (
    // Resolution protocols applied across all possible execution paths
    data: decay,
    cmd: decay
  )
  
  // If resolution occurs at 20ms, the logic executes (25ms), followed by 
  // 55ms of temporal padding to satisfy the 100ms invariant.
}
```

---

## 2. Entropic State-Based Routing (`match entropy`)

Within ICTL, variables undergo **Structural Decay** upon field access and transition to the **Consumed** state upon destructive read. Conventional branching mechanisms only route based on value; `match entropy` facilitates routing based on the physical integrity of memory.

### Operational Mechanics
- **`Valid(v)`**: The variable is intact and fully owned within the local arena. It supports movement, transmission, and replication.
- **`Decayed(v)`**: The variable remains present, but structural integrity is compromised (one or more fields consumed). Constituent fields may still be extracted, but the parent structure cannot be moved.
- **`Consumed`**: The variable has been removed from the local memory arena.

### Syntax Specification

```ictl
@0ms: {
  // Evaluation of 'user_identity' state post-reconciliation or channel reception
  
  match entropy(user_identity) {
    Valid(u):
      // Structural integrity maintained; variable supports movement.
      chan_send secure_transport(u)
      
    Decayed(u):
      // Structural integrity compromised; salvage residual fields.
      let identifier = u.id
      let recovered_state = struct { id = identifier, status = "reconstituted" }
      chan_send secure_transport(recovered_state)
      
    Consumed:
      // Variable state is terminal; execute fallback routine.
      System.Log(message="Fault: Identity state is terminal.")
  }
}
```

## 3. Semantic and Entropic Regulations

* **Analyzer Integration**: The Static Analyzer utilizes `match entropy` to prune causality trees. Inside a `Valid` block, movement is guaranteed; inside a `Consumed` block, any reference to the variable triggers a compile-time error.
* **Automated Reconciliation**: Because `match entropy` explicitly manages data presence, the Virtual Machine handles the entropic union at the block exit automatically.
- **Resolution Priority**: In the event that multiple `chan_recv` conditions in a `select` block achieve resolution at the same temporal coordinate, priority is determined by the order of specification.

---

### Architectural Significance
The `select` construct enables isolated timelines to function as high-performance, predictable event loops. The `match entropy` construct provides a first-class language primitive for interacting with decaying memory states, facilitating robust data recovery and systemic safety.
