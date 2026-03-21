# ICTL Routines & Temporal Contracts (`routine`)

This document outlines the formal requirements for code reuse and procedural execution within the ICTL Virtual Machine. It introduces **Temporal Contracts** and **Entropic Signatures** to ensure that invoking a routine does not introduce unpredictable delays or hidden memory consumption.

## 1. The Temporal Contract (`taking Nms`)

In ICTL, a routine cannot have an ambiguous execution time. It must declare a strict **Worst-Case Execution Time (WCET)** in its signature.

* **The Feature**: Every `routine` definition includes a `taking Nms` clause. This is a binding temporal contract.
* **The Logic**: 
  * If the routine's internal logic finishes *faster* than the contract, the VM automatically injects padding until the exact contract time is reached.
  * If the routine's internal logic *exceeds* the contract, the Static Analyzer will fail it at compile-time (or throw a `WatchdogBite` if the overrun depends on dynamic loops without max bounds).
* **Benefit**: The caller knows exactly how much `local_clock` will be consumed by the invocation, maintaining perfect timeline determinism.

## 2. Entropic Signatures

Passing arguments in ICTL requires explicit entropy management. A routine must declare exactly what it intends to do with the physical memory it receives.

* **`consume`**: The routine takes total ownership of the value. The caller's variable is marked as `Consumed` and can no longer be used.
* **`clone`**: The routine creates a deep copy of the value. The caller retains the original `Valid` variable. *Note: The computational cost of the clone is deducted from the routine's CPU budget.*
* **`decay`**: The routine requires a `Valid` struct and is permitted to destructively extract its fields. When the routine returns, the caller's variable is left in a `Decayed` state.
* **`peek`**: The routine requires read-only access. It cannot consume, clone, or decay the variable. This has zero entropic impact on the caller.

## 3. Syntax Proposal

### Defining a Routine

```ictl
// This routine guarantees it will take exactly 25ms to execute.
routine process_payment(consume auth_token, peek transaction_details) taking 25ms {
    
    // Auth token is consumed by this operation
    let is_valid = validate_token(auth_token) 
    
    if (is_valid) {
        let amount = transaction_details.amount // Peek allows safe field reading
        let receipt = NetworkRequest(domain="bank.ictl") // Costs 15ms
        yield receipt
    } else {
        yield "DECLINED"
        // VM automatically pads this branch to meet the 25ms contract
    } 
    reconcile (yield = first_wins)
}
```

### Invoking a Routine

Routine invocation uses the `call` keyword to explicitly flag a context shift and temporal jump.

```ictl
@0ms: {
    let token = "secure_abc123"
    let tx = struct { amount = 100, currency = "USD" }
    
    // The analyzer knows this call costs exactly 25ms.
    // It also marks 'token' as Consumed in the current arena.
    let result = call process_payment(token, tx)
    
    // Result is available at @25ms
    require System.Log(message=result)
    
    // ERROR: Static Analyzer prevents this.
    // let retry = call process_payment(token, tx) -> 'token' is Consumed!
}
```

## 4. Semantic Rules & Constraints

* **Purity**: Routines cannot contain `split` or `merge` statements that affect the caller's timeline structure. They operate entirely within an ephemeral, isolated sub-arena that merges its `yield` value back to the caller upon completion.
* **No Global Time Coordinates**: Routines cannot use `@10ms:` blocks. They are relativistic procedures; their time is entirely relative to the moment they are called (`@+Xms`).
* **Nested Contracts**: If Routine A calls Routine B (which takes 10ms), the Static Analyzer adds 10ms to Routine A's internal WCET calculation to ensure it doesn't breach its own temporal contract.

---

### Why this fits ICTL perfectly:
By forcing the developer to explicitly state the temporal cost (`taking 25ms`) and the entropic impact (`consume`, `peek`), you retain 100% of the Static Analyzer's power across procedural boundaries. Code reuse no longer creates black holes in your causal tracking.