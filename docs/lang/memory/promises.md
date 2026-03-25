# Promises and Deferred Effects

This document details the `defer` and `await` constructs in ICTL, which provide a deterministic model for asynchronous-style operations.

---

## 1. Deferred Expressions (`defer`)

The `defer` keyword initializes an asynchronous-style effect that will resolve at a future point in the timeline.

### Syntax
```ictl
let <identifier> = defer <capability>(<params>) deadline <amount>ms
```

**Semantics:**
- **`Pending` State**: The target variable enters the `Pending` state in the current branch's arena.
- **Clock Independence**: Initializing a `defer` expression does not advance the `local_clock`.
- **Latency and Deadline**: Each promise is assigned a `ready_at` and `deadline_at` timestamp relative to the current `local_clock`.

---

## 2. Promise Resolution (`await`)

The `await` statement is used to force the resolution of a `Pending` value.

### Syntax
```ictl
await(<identifier>)
```

**Behavior:**
1. **Clock Advance**: If the current `local_clock` is less than the promise's `ready_at` time, the `local_clock` is advanced to `ready_at`. This consumes the branch's CPU budget.
2. **Resolution Check**:
   - If the new `local_clock` is less than or equal to the `deadline_at`, the value resolves to `Valid` with the capability's payload.
   - If the `local_clock` has already passed the `deadline_at`, the value transitions directly to `Consumed` (failure).
3. **No-op**: If the variable is already `Valid`, `Decayed`, or `Consumed`, `await` has no effect.

---

## 3. Branching on State (`match entropy`)

Use `match entropy` to safely handle the different states of a promise.

### Example
```ictl
match entropy(ds) {
  Pending(p):
    // Promise is still unresolved
    let status = "waiting"
  Valid(v):
    // Promise resolved successfully; 'v' is the payload
    let status = v
  Consumed:
    // Promise timed out or was already consumed
    let status = "timeout"
}
```

---

## 4. Deterministic Simulation

In ICTL's research runtime, external effects (like network requests) are simulated deterministically:
- `ready_at` is calculated based on a provided or default `latency` parameter.
- The outcome is strictly bound to the `local_clock` and `deadline` contract, ensuring the program's behavior remains identical across all execution runs.
