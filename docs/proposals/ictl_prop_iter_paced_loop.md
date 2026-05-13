# Proposal: Entropic Iteration and Paced Loops

This document specifies the formal requirements for collection iteration within the Isolate Concurrent Temporal Language (ICTL). It introduces **Destructive Iteration**, **Temporal Pacing**, and **Scatter-Gather Branching** to ensure that iterative constructs remain entropically safe and temporally deterministic.

## 1. The Entropic Iterator (`consume` vs. `clone`)
In conventional programming languages, iterative access is typically a passive read operation. Within the ICTL framework, memory is entropic, necessitating an explicit declaration of the entropic impact of iteration on the source structure.

* **Structural Requirement**: `for` loops must explicitly designate whether they are **consuming** the collection (destructive) or **cloning** the constituent elements (resource-intensive).
* **Syntax A: Destructive Iteration (`consume`)**
    ```ictl
    for item consume processing_queue {
        // 'item' is fully owned by the active iteration scope.
        // 'processing_queue' undergoes structural decay during iteration.
    }
    // ERROR: 'processing_queue' is in the Consumed state and is inaccessible.
    ```
* **Syntax B: Non-Destructive Iteration (`clone`)**
    ```ictl
    for item clone reference_collection {
        // 'item' represents a deep replication. 
        // The VM deducts the temporal cost of replication from the CPU budget.
    }
    // 'reference_collection' remains in the Valid state.
    ```
* **Static Analysis**: The analyzer flags a collection as `Decayed` or `Consumed` immediately upon entry into a `consume` loop. 

## 2. Deterministic Temporal Pacing (`pacing`)
Loops processing data streams frequently require execution at a fixed frequency. ICTL implements this requirement at the Virtual Machine level through the `pacing` keyword.

* **Structural Requirement**: The `pacing` keyword enforces an exact, deterministic duration for each iteration, utilizing the principle of **Temporal Equalization** (padding).
* **Execution Logic**: If the loop body executes in duration $t_{body}$ and the pacing is specified as $T_{pacing}$, the VM applies $T_{pacing} - t_{body}$ milliseconds of temporal padding. If $t_{body} > T_{pacing}$, the Static Analyzer triggers a compile-time Worst-Case Execution Time (WCET) violation.
* **Technical Implementation Example**:
    ```ictl
    // A 10Hz processing cycle (100ms per iteration)
    for packet consume network_stream pacing 100ms {
        let parsed = decode(packet) // Execution duration: ~12ms
        // VM automatically applies 88ms of temporal padding.
    }
    ```

## 3. Entropic Scatter-Gather Architecture (`split_map`)
Within the ICTL parallel execution paradigm, sequential iteration is frequently insufficient. The `split_map` construct facilitates the transformation of a collection into a matrix of parallel timelines.

* **Functional Requirement**: `split_map` accepts a collection and initializes a distinct timeline branch for each element, executing them concurrently within isolated memory arenas prior to deterministic reconciliation.
* **Execution Mechanics**: The construct consumes the parent collection, initializes $N$ branches, executes the logic block, and applies a universal `reconcile` protocol to aggregate results into a structured collection.
* **Technical Implementation Example**:
    ```ictl
    // 'batch_operations' represents a collection of 5 elements.
    // This initializes 5 parallel execution timelines.
    let results = split_map task consume batch_operations {
        let processed = intensive_computation(task) // Execution within an isolated arena
        yield processed
    } reconcile (gather) // Aggregates yielded values into a new array structure
    ```

---

## Detailed Specification: ICTL Iterative Constructs

| Feature | Conventional `for` | ICTL Entropic `for` |
| :--- | :--- | :--- |
| **Collection State** | Invariant | **Explicitly Consumed or Replicated** |
| **Iteration Timing** | Variable (Jitter) | **Paced or Deterministically Bounded** |
| **Concurrency** | Sequential only | **Native `split_map` timeline initialization** |
| **Execution Cost** | Opaque | **Statically analyzed WCET constraints** |

---

### Architectural Impact
This proposal solidifies the ICTL memory and temporal models. By requiring explicit `consume` or `clone` designations, hidden memory overhead is eliminated. The integration of the `pacing` keyword facilitates the creation of precise temporal execution cycles, eliminating iteration drift and ensuring systemic determinism.
