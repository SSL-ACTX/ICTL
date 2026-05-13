# ICTL Core Semantic Model

This document specifies the fundamental semantic regulations governing the Isolate Concurrent Temporal Language (ICTL).

---

## 1. Entropic Memory Model

The Entropic memory model constitutes the foundational architecture of ICTL. It conceptualizes values not as static data structures, but as dynamic energy states that undergo evolution, movement, and decay during the computational lifecycle.

### Entropic States
- **Valid**: The value is owned by the active branch and is fully accessible for computation.
- **Pending**: The value represents a promise (e.g., resulting from a `defer` operation) that will be resolved at a future `local_clock` coordinate.
- **Decayed**: The value (typically a structure) has undergone partial consumption of its internal fields. The parent structure is no longer "sealed" and cannot be moved or transmitted as a unified entity.
- **Consumed**: The value has been destructively read (e.g., via a `let` assignment or `chan_send` operation) and is no longer present within the memory arena.

### Consumption Regulations
1. **Movement by Default**: Assignments (e.g., `let a = b`) transfer ownership of `b` to `a`. Subsequently, `b` transitions to the `Consumed` state.
2. **Structural Decay**: Accessing a sub-field `s.f` consumes `f` and transitions the parent structure `s` to the `Decayed` state.
3. **Explicit Replication**: Reusing a value without consumption requires the `clone(x)` operation, which incurs a deterministic temporal cost.

---

## 2. Deterministic Temporal Execution

ICTL enforces rigorous, predictable execution durations for all computational operations.

### Local Temporal Clock and Resource Budget
- Every execution branch maintains a `local_clock`, measured in milliseconds.
- Every instruction possesses a deterministic temporal cost (base cost: 1ms).
- Branches are initialized with a defined `cpu_budget_ms`. Exceeding this budget triggers a runtime `BudgetExhausted` fault.

### Pacing and Deterministic Padding
- Iterative constructs (e.g., `for` loops) and routine invocations enforce temporal contracts.
- **Deterministic Padding**: If an execution block completes prior to its contracted duration (e.g., `taking 20ms`), the Stack-based Temporal Virtual Machine (STVM) automatically pads the `local_clock` to satisfy the contract, ensuring that execution duration is independent of the source environment.
- **Temporal Watchdogs**: If an execution block exceeds its allocated duration, a `WatchdogBite` is triggered, facilitating the execution of recovery logic.

---

## 3. Timeline Isolation and Concurrency

Concurrency in ICTL is modeled through isolated **Timelines** (branches).

### Split and Reconciliation
- **`split`**: Generates child timelines initialized with a snapshot of the parent arena and temporal clock.
- **`merge`**: Recombines child timelines into the parent context. Conflicts (e.g., concurrent modification or consumption of shared state) must be resolved via explicit `reconcile` or `resolving` protocols.

---

## 4. Isochronous Scheduling Matrix

For applications requiring high-precision timing, ICTL supports the **Isochronous Matrix** scheduling model.

- **Temporal Slices**: The `slice Nms` primitive establishes a fixed tick frequency.
- **Phase Commits**: `loop tick` blocks ensure that all channel operations occur within deterministic phases; transmissions are buffered until the tick boundary, and receptions read from the buffer of the preceding tick.

---

## 5. Causal Reversion and Paradox Mitigation

ICTL facilitates high-assurance state recovery through the `anchor` and `rewind_to` primitives.

### Temporal Integrity Maintenance
- **Temporal Restoration**: Reverting to an anchor restores the branch to the precise `local_clock` coordinate of that anchor.
- **State Restoration**: The memory arena is restored to the exact snapshot recorded at the anchor point.

### Paradox Prevention Mechanisms
To maintain temporal consistency, the STVM prevents the occurrence of **Causal Paradoxes**:
1. **Unconsumed Side Effects**: A branch may rewind past a `chan_send` only if the transmitted message hasn't been consumed by another branch. The VM facilitates an automated "un-send" operation.
2. **Causal Outflow Locking**: If a transmitted message has already been consumed by another branch, the source branch is "causally locked" to that event. Any attempt to rewind past this point triggers a `Causal Paradox` error.
3. **Automated Reception Reversal**: If a branch rewinds past a `chan_recv`, the message is automatically restored to the channel buffer to preserve systemic data integrity.
