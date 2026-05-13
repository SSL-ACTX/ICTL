# Proposal: ICTL Routines and Temporal Contracts

This document specifies the formal requirements for procedural execution and code reuse within the Isolate Concurrent Temporal Language (ICTL). It introduces **Temporal Contracts** and **Entropic Signatures** to ensure that routine invocation does not introduce non-deterministic latency or opaque memory consumption.

## 1. The Temporal Execution Contract (`taking Nms`)

Within the ICTL framework, a routine must possess an invariant execution duration, specified as a strict **Worst-Case Execution Time (WCET)**.

* **Structural Requirement**: Every `routine` specification must incorporate a `taking Nms` clause, forming a binding temporal contract.
* **Execution Logic**: 
  * If the routine's logic completes in less than the contracted duration, the Stack-based Temporal Virtual Machine (STVM) applies deterministic padding until the contract is satisfied.
  * If the routine's logic exceeds the specified duration, the Static Analyzer rejects the code at compile-time (or triggers a `WatchdogBite` in the case of dynamic loop overruns).
* **Technical Benefit**: The caller is guaranteed an exact advancement of the `local_clock` upon invocation, preserving systemic timeline determinism.

## 2. Entropic Signature Specifications

Argument passing in ICTL requires explicit management of entropic state. Routines must formally declare the intended effect on the memory arena of the caller.

* **`consume`**: The routine assumes full ownership of the value. The caller's variable is marked as `Consumed` and becomes inaccessible.
* **`clone`**: The routine receives a deep replication of the value, while the caller retains the original. The temporal cost of the replication is deducted from the routine's allocated temporal budget.
* **`decay`**: The routine requires a `Valid` structure and is permitted to perform destructive field extraction. Upon routine termination, the caller's variable remains in the `Decayed` state.
* **`peek`**: Provides read-only access for inspection without modifying the entropic state of the caller's memory arena.

## 3. Syntax Specification

### Routine Specification

```ictl
// This routine maintains an invariant execution duration of exactly 25ms.
routine process_transaction(consume authentication_token, peek transaction_metadata) taking 25ms {
    
    // The authentication_token is consumed by this operation
    let is_valid = validate_token(authentication_token) 
    
    if (is_valid) {
        let amount = transaction_metadata.amount // Peek facilitates safe field inspection
        let receipt = NetworkRequest(domain="financial_gateway.ictl") // Deterministic cost: 15ms
        yield receipt
    } else {
        yield "TRANSACTION_DECLINED"
        // STVM applies padding to satisfy the 25ms execution contract
    } 
    reconcile (yield = first_wins)
}
```

### Invocation Protocol

Routine invocation utilizes the `call` primitive to specify a formal context transition and temporal advancement.

```ictl
@0ms: {
    let token = "secure_identifier_xyz"
    let tx = struct { amount = 100, currency = "USD" }
    
    // The analyzer identifies this invocation as having a 25ms temporal cost.
    // The 'token' variable is marked as Consumed within the active arena.
    let result = call process_transaction(token, tx)
    
    // The result is available at temporal coordinate @25ms
    System.Log(message=result)
    
    // ERROR: Static Analysis prevents subsequent access to Consumed variables.
    // let retry = call process_transaction(token, tx) // 'token' is Consumed
}
```

## 4. Semantic Regulations and Operational Constraints

* **Isolation Invariant**: Routines are prohibited from containing `split` or `merge` operations that modify the caller's timeline architecture. They operate within an isolated sub-arena, merging only the `yield` value back to the caller.
* **Relativistic Temporal Markers**: Routines may not utilize absolute temporal coordinates (e.g., `@10ms:`). Their execution is entirely relative to the point of invocation (`@+Xms`).
* **Nested Contract Enforcement**: In the event that Routine A invokes Routine B (which has a 10ms contract), the Static Analyzer incorporates this 10ms into Routine A's internal WCET calculation to ensure contract compliance.

---

### Conceptual Alignment
By requiring explicit specification of temporal cost (`taking 25ms`) and entropic impact (`consume`, `peek`), the integrity of the Static Analyzer is maintained across procedural boundaries. This ensures that code reuse does not result in opaque causal tracking or temporal non-determinism.
