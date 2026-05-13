# Proposal: Relativistic I/O and Entropic Promises

This document specifies the formal requirements for managing external, non-deterministic input/output (I/O) within the Isolate Concurrent Temporal Language (ICTL) Virtual Machine. It introduces the **Pending** entropic state and the protocol for **Timeline Suspension** to interface with external environments without compromising internal causal determinism.

## 1. The I/O Determinism Challenge
Within the ICTL framework, every operation must possess a predictable temporal cost. External I/O operations inherently violate this principle, as their Worst-Case Execution Time (WCET) is frequently unbounded. Consequently, I/O operations cannot be evaluated synchronously and must be decoupled from the `local_clock` accounting.

## 2. The `Pending` Entropic State
To manage data that is scheduled for future resolution, the `EntropicState` model is expanded:
- **`Valid(payload)`**: Data is fully resolved and accessible.
- **`Decayed(fields)`**: Data is partially consumed.
- **`Consumed`**: Terminal state; data is inaccessible.
- **`Pending(promise_id)`**: Data is currently undergoing resolution across the causality boundary of the external environment.

## 3. The `defer` Primitive and `deadline` Execution Contract
External operations are initiated via the `defer` primitive. This immediately returns a variable in the `Pending` state, incurring a deterministic cost of 1ms. The actual execution occurs outside the Virtual Machine's temporal accounting framework.

Every `defer` operation must incorporate a `deadline` to establish a formal boundary for non-deterministic execution.

```ictl
@0ms: {
    // Deterministic cost: 1ms. 'user_data' is initialized in the Pending state.
    // The VM schedules the background execution of the NetworkFetch operation.
    let user_data = defer System.NetworkFetch(url="api.ictl/user/1") deadline 200ms
    
    // Timeline execution continues immediately
    let localized_computation = 50 * 20
}
```

## 4. State Synchronization Protocols (`await` vs. `match entropy`)

A timeline cannot access the internal structure of a `Pending` variable. To utilize the data, the timeline must synchronize with the resolution of the external event.

### Protocol A: Deterministic Synchronization (`await`)
The `await` primitive suspends the `local_clock` of the active branch, effectively detaching it from the `global_clock` until the promise is resolved or the deadline is reached. 
- Upon data arrival, the `local_clock` is advanced to reflect the elapsed duration.
- If the `deadline` is exceeded, the `await` operation results in a transition to the `Decayed` or `Consumed` state, indicating a temporal timeout.

```ictl
@worker: {
    let dataset = defer System.DatabaseQuery(query="SELECT *") deadline 500ms
    
    // Execution suspends here. If resolution takes 40ms, local_clock is advanced by 40ms.
    // If resolution takes 600ms, execution terminates at the 500ms deadline.
    await(dataset) 
}
```

### Protocol B: Non-Blocking Entropic Routing
As an alternative to suspension, a timeline may utilize the `match entropy` construct to verify the resolution status of the data, treating temporal progression as a physical state of memory.

```ictl
@worker: {
    let payload = defer System.NetworkFetch(url="api.data") deadline 1000ms
    
    // Concurrent execution for 50ms
    
    match entropy(payload) {
        Pending(p):
            System.Log(message="Resolution pending. Executing fallback logic.")
        Valid(data):
            // Data resolved prior to synchronization.
            let identifier = data.id
        Consumed:
            // The deadline was exceeded, resulting in resolution failure.
            System.Log(message="Temporal deadline exceeded.")
    }
}
```

## 5. Virtual Machine Implementation Specifications
1.  **Capability Interface**: The `capability_handlers` within the VM must support asynchronous execution (e.g., via Rust `Future` objects) for `defer` operations, avoiding blocking the primary VM execution thread.
2.  **Temporal Desynchronization**: When an execution branch invokes `await`, it enters a `Suspended` state. The VM's `global_clock` continues to advance for parallel branches. Upon I/O resolution, the suspended branch is re-integrated into the active execution queue.

---

### Architectural Alignment
By conceptualizing asynchronous data as an **Entropic State** (`Pending`), non-deterministic I/O is integrated into the existing routing and memory safety frameworks of ICTL. This architectural approach transforms network latency into a manageable memory-state transition, consistent with the fundamental design principles of the language.
