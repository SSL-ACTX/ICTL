# Proposal: Entropic Garbage Collection (EGC)

This document specifies the formal requirements for memory reclamation within the Isolate Concurrent Temporal Language (ICTL) Virtual Machine. It introduces **Zero-Jitter Deallocation**, utilizing entropic state tracking to facilitate deterministic memory reclamation without the non-deterministic pauses associated with conventional Garbage Collection (GC) mechanisms.

## 1. Limitations of Conventional Memory Management
Traditional memory management strategies are incompatible with ICTL's requirement for absolute temporal determinism:
* **Tracing Garbage Collection**: Non-deterministic "stop-the-world" pauses introduce significant temporal jitter.
- **Reference Counting**: Cascading deallocations can result in unpredictable execution delays during variable disposal.
- **Manual Management**: Inherently unsafe and prone to memory leaks and use-after-free vulnerabilities.

## 2. The EGC Architectural Philosophy
Within the ICTL framework, the lifecycle of a variable is mathematically coupled to its entropic state. When a variable's entropy reaches a terminal state (i.e., it is `Consumed` across all execution timelines), its physical memory is deterministically reclaimed.

As the `EntropicAnalyzer` monitors every consumption, split, and merge operation, EGC operates without the need for a runtime scanner. The compiler injects explicit `dealloc` instructions into the VM bytecode at the precise temporal coordinate of a value's expiration.

## 3. Foundational Pillars of EGC

### I. Static Deallocation Injection
Memory is reclaimed immediately upon variable consumption within a standard execution block. 
* **The Regulation**: A `consume` operation (e.g., transmission via a channel or passing to a destructive routine) serves as an implicit, safe deallocation trigger. The Stack-based Temporal Virtual Machine (STVM) removes the payload from the local `Arena` with a deterministic temporal cost of 1ms.
* **Decay Management**: When a structure undergoes `Structural Decay`, the parent container is logically invalidated, but physical memory remains allocated until the final constituent field is consumed.

### II. Arena-Level Bulk Reclamation
Upon the termination of a micro-timeline or parallel execution branch, individual variable tracing is unnecessary. The comprehensive `Arena` is reclaimed at the system level with $O(1)$ complexity.
- **Speculation Failure (`collapse`)**: Upon speculation failure, the associated transient arena is immediately reclaimed. 
- **Timeline Reconciliation (`merge`)**: During reconciliation, variables not explicitly yielded or reconciled are immediately dropped. Only variables designated within the `reconcile` block are transferred to the parent arena.

### III. Commit-Horizon Snapshot Pruning
The `anchor` keyword introduces complexity by creating acausal snapshots; memory cannot be reclaimed if it is `Consumed` in the active timeline if the VM requires the ability to `reset` to a previous state.
* **The Regulation**: Memory associated with an `anchor` enters a **Suspended Entropic State**.
* **The Horizon**: Upon execution passing a `commit { ... }` block, all preceding anchors within that timeline are invalidated. At this precise temporal coordinate, the STVM bulk-reclaims the snapshots, freeing host memory resources.

---

## 4. Operational Analysis: EGC in Execution

The following demonstrates the mapping of source code to implicit EGC operations.

### ICTL Implementation
```ictl
@0ms: {
    let large_dataset = NetworkRequest(domain="data_repository.ictl") 
    
    speculate (max 50ms) {
        anchor parse_initialization
        
        // 'large_dataset' is logically consumed
        let parsed = call process_data(consume large_dataset)
        
        if (parsed == "corrupt") {
            collapse // Aborts and restores 'large_dataset'
        }
        
        // Anchor is invalidated. Memory for 'large_dataset' is reclaimed.
        commit (result = parsed) 
    } fallback {
        let result = "execution_failed"
    }
}
```

### STVM Execution Path (Implicit Operations)
1. **`@0ms`**: `large_dataset` is allocated within the Primary Arena.
2. **`speculate`**: A Micro-Arena is initialized.
3. **`anchor`**: The Primary Arena is snapshotted (entering a Suspended State).
4. **`call`**: `large_dataset` is consumed within the routine. Host memory remains allocated due to the existence of the `parse_initialization` anchor.
5. **`commit`**: 
    - The Micro-Arena is merged into the Primary Arena. 
    - The `parse_initialization` anchor is invalidated. 
    - **[EGC Trigger]**: The VM identifies that `large_dataset` has reached zero entropy with no remaining anchors, resulting in physical memory reclamation.

---

## 5. Architectural Implementation Impact
The implementation of EGC requires the following updates to the VM architecture:
1.  **Arena Architecture**: Update the `Arena` implementation to utilize efficient bulk reclamation strategies.
2.  **Commit Logic**: Ensure that the `commit` operation explicitly triggers the reclamation of invalidated `arena_snapshot` data.
3.  **Semantic Validation**: Incorporate a final analysis pass to ensure that no variables remain in a `Valid` or `Decayed` state at program termination without explicit consumption.

---

### Architectural Significance
EGC eliminates the unpredictable latency spikes characteristic of traditional garbage-collected languages. It provides memory safety comparable to modern systems languages while utilizing a concurrency model designed for predictable, bulk memory reclamation at timeline boundaries.
