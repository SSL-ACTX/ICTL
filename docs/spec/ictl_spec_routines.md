# Routine Temporal Contracts and Execution

This document formalizes the syntax and semantics of the `routine` and `call` constructs within the Isolate Concurrent Temporal Language (ICTL).

---

## 1. Routine Specification (`routine`)

A routine defines a reusable procedure governed by an explicit temporal and entropic execution contract.

### Formal Syntax
```ictl
routine <name>(<param_mode> <identifier>, ...) taking (<amount>ms | _) {
  <statements>
  [yield <expression>]
}
```

### Parameter Passing Protocols
- **`consume`**: Default behavior. The caller's value is moved into the routine's scope and is marked as `Consumed` within the caller's memory arena.
- **`clone`**: The caller retains the original value; the routine receives a deep replication, incurring a deterministic cloning cost.
- **`decay`**: The value is moved, and in the case of structured data, the original value in the caller's arena is transitioned to the `Decayed` state.
- **`peek`**: Provides read-only access for inspection; the caller's arena state remains unmodified.

---

## 2. Temporal Execution Contracts (`taking`)

The `taking` clause specifies the **Worst-Case Execution Time (WCET)** for the routine.

- **Explicit Temporal Specification**: `taking 20ms`. The Stack-based Temporal Virtual Machine (STVM) guarantees the routine execution occupies exactly 20ms. If completion occurs prematurely, deterministic padding is applied. If the duration is exceeded, a `WatchdogBite` is triggered.
- **Inferred Temporal Specification**: `taking _`. The static analyzer computes the maximum execution cost across all code paths and establishes the contract automatically.

---

## 3. Invocation Protocol (`call`)

Routines are invoked utilizing the `call` primitive.

### Formal Syntax
```ictl
let <result> = call <name>(<arguments>)
```

### Execution Semantics
1. Arguments undergo evaluation and are moved or cloned according to the routine's formal declaration.
2. The caller's `local_clock` is advanced by the routine's `taking_ms` contract.
3. The initial `yield` value, if present, is returned to the caller's context.

---

## 4. Yield and Return Mechanisms

- **`yield <expression>`**: Emits a value from the routine and terminates its execution for the current invocation.
- **Void Routines**: In the absence of a `yield` execution, the routine returns a `void` payload.

---

## 5. Foundational Constraint Rules

1. **Execution Isolation**: Routines are prohibited from containing `@...` temporal markers, or `split`, `merge`, and `isolate` blocks.
2. **Temporal Determinism**: Every execution path within a routine must satisfy the `taking` contract. Static analysis rejects any routine where a path's cost exceeds the specified `taking_ms`.
