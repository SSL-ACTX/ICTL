# ICTL Advanced Routing (`select` / `match entropy`)

This document outlines the formal requirements for advanced routing control flows in the ICTL Virtual Machine. It introduces **Temporal Multiplexing** for channels and **State-Based Routing** for entropic memory validation.

## 1. Relativistic `select` (Temporal Multiplexing)

With the introduction of Entropic Channels, timelines often need to listen to multiple data streams concurrently. Traditional languages use `select` or `poll` for this, but these introduce unpredictable blocking times. ICTL’s `select` is strictly bounded by the `global_clock` and enforces the Padding Rule.

### The Mechanics
* **Temporal Bound**: A `select` block must declare a `max` wait time. 
* **Deterministic Padding**: Regardless of which channel resolves first, or if the block times out, the total time cost of the `select` block is mathematically fixed to the `max` value plus the WCET of the chosen branch.
* **Destructive Read**: Whichever case successfully receives data destructively consumes it from the channel. Unselected channels remain untouched.

### Syntax Proposal

```ictl
@0ms: {
  // Wait for data from either the telemetry pipe or the command pipe
  select (max 100ms) {
    case data = chan_recv(telemetry_pipe):
      let processed = parse_telemetry(data) // WCET: 10ms
      
    case cmd = chan_recv(command_pipe):
      let executed = run_command(cmd)       // WCET: 25ms
      
    timeout:
      // Mandatory block executed if 100ms passes without a receive
      require System.Log(message="Both channels silent. Proceeding.")
      
  } reconcile (
    // Reconcile rules apply across all cases and the timeout
    data: decay,
    cmd: decay
  )
  
  // If 'command_pipe' triggered at 20ms, the VM processes it (25ms), 
  // and then pads the timeline by 55ms, ensuring the block ALWAYS costs 100ms.
}
```

---

## 2. Entropic `match` (State-Based Routing)

In ICTL, variables undergo **Structural Decay** when their fields are accessed, and are completely removed from the arena when **Consumed**. Traditional `match` / `switch` statements only route based on a variable's *value*. 

`match entropy()` is a unique ICTL control flow that routes logic based on the physical integrity of the memory itself, allowing timelines to dynamically recover from receiving damaged or consumed data.

### The Mechanics
* **`Valid(v)`**: The variable exists and its structural seal is completely intact. It can be fully moved, sent across channels, or cloned.
* **`Decayed(v)`**: The variable exists, but one or more of its fields have been consumed. The parent struct cannot be moved, but its remaining intact fields can still be extracted.
* **`Consumed`**: The variable no longer exists in the local arena.

### Syntax Proposal

```ictl
@0ms: {
  // Assume 'user_struct' was passed in from a merge or channel
  
  match entropy(user_struct) {
    Valid(u):
      // The struct is perfect. We can safely move it to another timeline.
      chan_send safe_pipe(u)
      
    Decayed(u):
      // The struct's seal is broken. We can't move it, but we can salvage 
      // remaining fields to construct a new valid struct.
      let salvaged_id = u.id
      let rebuilt = struct { id = salvaged_id, status = "recovered" }
      chan_send safe_pipe(rebuilt)
      
    Consumed:
      // The variable was already used. We branch to a fallback routine.
      require System.Log(message="Fault: Expected user_struct but arena was empty.")
  }
}
```

## 3. Semantic & Entropic Rules

* **Analyzer Integration**: The static analyzer uses `match entropy` to prune its causality trees. Inside the `Valid` block, the analyzer guarantees the variable can be moved. Inside the `Consumed` block, the analyzer will throw a compile-time error if the programmer attempts to reference the variable at all.
* **No Reconciliation Required for Entropy Match**: Because `match entropy` specifically deals with the presence or absence of data, the VM automatically handles the entropic union at the end of the block.
* **Select Conflict Resolution**: If multiple `chan_recv` conditions in a `select` block resolve at the exact same millisecond of the `global_clock`, priority is evaluated top-to-bottom.

---

### Why this fits ICTL perfectly:
The `select` block gives your isolated timelines the ability to act as high-performance, predictable event loops. The `match entropy()` block fully realizes the Phase 10 Structural Decay feature you built earlier, giving developers a safe, first-class language construct to interact with decaying memory.