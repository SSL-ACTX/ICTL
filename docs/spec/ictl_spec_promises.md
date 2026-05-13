# Temporal Promises and Deferred Execution

This document specifies the `defer` and `await` constructs within the Isolate Concurrent Temporal Language (ICTL), which provide a deterministic framework for asynchronous-style operations.

---

## 1. Deferred Expressions (`defer`)

The `defer` keyword initializes an asynchronous-style effect that is scheduled for resolution at a future temporal coordinate.

### Formal Syntax
```ictl
let <identifier> = defer <capability>(<params>) deadline <amount>ms
```

**Execution Semantics:**
- **`Pending` State Transition**: The target variable enters the `Pending` state within the active branch's memory arena.
- **Temporal Clock Independence**: The initialization of a `defer` expression does not result in the advancement of the `local_clock`.
- **Latency and Deadline Specifications**: Every promise is assigned a `ready_at` and `deadline_at` timestamp relative to the current `local_clock`.

---

## 2. Promise Resolution Protocol (`await`)

The `await` statement is utilized to enforce the resolution of a `Pending` value.

### Formal Syntax
```ictl
await(<identifier>)
```

**Behavioral Logic:**
1. **Temporal Clock Advancement**: If the current `local_clock` is inferior to the promise's `ready_at` timestamp, the `local_clock` is advanced to `ready_at`. This operation consumes the branch's allocated CPU budget.
2. **Resolution Verification**:
   - If the updated `local_clock` is less than or equal to the `deadline_at`, the value transitions to the `Valid` state, incorporating the capability's payload.
   - If the `local_clock` exceeds the `deadline_at`, the value transitions to the terminal `Consumed` state, indicating a resolution failure.
3. **Null Operation**: If the variable already occupies the `Valid`, `Decayed`, or `Consumed` state, the `await` operation has no effect.

---

## 3. Entropic State Branching

The `match entropy` construct is employed to handle the distinct states of a temporal promise with semantic safety.

### Technical Implementation Example
```ictl
match entropy(ds) {
  Pending(p):
    // Promise remains unresolved
    let status = "awaiting_resolution"
  Valid(v):
    // Promise resolved successfully; 'v' represents the payload
    let status = v
  Consumed:
    // Promise exceeded deadline or underwent prior consumption
    let status = "temporal_timeout"
}
```

---

## 4. Deterministic Behavioral Simulation

Within the ICTL research runtime, external effects—such as network operations—are simulated deterministically:
- The `ready_at` timestamp is calculated based on specified or default latency parameters.
- The outcome is strictly bound to the `local_clock` and the `deadline` contract, ensuring that program behavior remains invariant across disparate execution environments.
