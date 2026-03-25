# Routine Temporal Contracts

This document formalizes the syntax and semantics of `routine` and `call` constructs in ICTL.

---

## 1. Routine Definition (`routine`)

A routine defines a reusable procedure with an explicit temporal and entropic contract.

### Syntax
```ictl
routine <name>(<param_mode> <identifier>, ...) taking (<amount>ms | _) {
  <statements>
  [yield <expression>]
}
```

### Parameter Modes
- **`consume`**: Default. The caller's value is **moved** and marks it `Consumed` in the caller.
- **`clone`**: Caller keeps the original; routine gets a deep copy. Incurs deterministic cloning cost.
- **`decay`**: Moves the value, but if it's a struct, it's marked as `Decayed` in the caller's arena.
- **`peek`**: Read-only access for inspection; caller's arena state is unchanged.

---

## 2. Temporal Contracts (`taking`)

The `taking` clause defines the **Worst-Case Execution Time (WCET)** for the routine.

- **Explicit Timing**: `taking 20ms`. The VM guarantees the routine call takes exactly 20ms. If it finishes early, it **pads** the local clock. If it overruns, it triggers a `WatchdogBite`.
- **Inferred Timing**: `taking _`. The static analyzer calculates the maximum cost of all code paths and sets the contract automatically.

---

## 3. Invocation (`call`)

Routines are executed using the `call` keyword.

### Syntax
```ictl
let <result> = call <name>(<arguments>)
```

**Semantics:**
1. Arguments are evaluated and moved/cloned based on the routine's declaration.
2. The caller's `local_clock` is advanced by the routine's `taking_ms` contract.
3. The first `yield` value (if any) is returned to the caller.

---

## 4. Yield and Return

- **`yield <expression>`**: Emits a value from the routine and concludes its execution for the current call.
- **Void Routines**: If no `yield` is executed, the routine returns a `void` payload.

---

## 5. Constraint Rules

1. **Isolation**: Routines cannot contain `@...` timeline entries, `split`, `merge`, or `isolate` blocks.
2. **Determinism**: Every path in a routine must satisfy the `taking` contract. Static analysis rejects routines where a path's cost exceeds the declared `taking_ms`.
