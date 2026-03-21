# ICTL Proposal: Entropic Iteration & Paced Loops

This document outlines the formal requirements for collection iteration in the ICTL Virtual Machine. It introduces **Destructive Iteration**, **Temporal Pacing**, and **Scatter-Gather Branching** to ensure that loops remain entropically sound and temporally deterministic.

## 1. The Entropic Iterator (`consume` vs `clone`)
In traditional languages, `for (let x of array)` is a passive read. In ICTL, memory is entropic. Reading a collection must explicitly declare its entropic impact on the source structure.

* **The Feature**: `for` loops must explicitly state whether they are **consuming** the collection (destructive) or **cloning** the elements (resource-intensive).
* **Syntax 1: Destructive Loop (`consume`)**
    ```ictl
    for item consume payload_queue {
        // 'item' is fully owned by this iteration.
        // 'payload_queue' undergoes Structural Decay during the loop.
    }
    // ERROR: 'payload_queue' is now Decayed and cannot be referenced.
    ```
* **Syntax 2: Non-Destructive Loop (`clone`)**
    ```ictl
    for item clone reference_list {
        // 'item' is a deep clone. 
        // The VM automatically deducts CPU budget/time for the clone operation.
    }
    // 'reference_list' remains intact.
    ```
* **Static Analysis**: The Analyzer will flag a collection as `Decayed` immediately upon entry into a `consume` loop. 

## 2. Temporal Pacing (The `pacing` Keyword)
Loops processing streams or queues often need to operate at specific frequencies. Standard languages require manual `sleep()` calls, which introduce drift. ICTL handles this at the VM level.

* **The Feature**: The `pacing` keyword forces each iteration of a loop to take an exact, deterministic amount of time, leveraging the previously established **Temporal Equalization** (Padding) rule.
* **The Logic**: If the loop body executes in $t_{body}$ and the pacing is set to $T_{pace}$, the VM injects $T_{pace} - t_{body}$ milliseconds of temporal padding. (If $t_{body} > T_{pace}$, the Analyzer throws a compile-time WCET error).
* **Syntax Example**:
    ```ictl
    // A 10Hz processing loop (100ms per iteration)
    for packet consume network_stream pacing 100ms {
        let parsed = decode(packet) // Takes ~12ms
        // VM automatically pads the remaining 88ms.
    }
    ```

## 3. Entropic Scatter-Gather (`split_map`)
Because ICTL thrives on parallel timelines, sequential iteration isn't always the right paradigm. We need a way to turn a collection of data into a matrix of parallel timelines.

* **The Feature**: `split_map` takes a collection and spawns a distinct timeline branch for *each element*, executing them concurrently in isolated arenas, and then deterministically merging them back.
* **The Mechanics**: It consumes the parent collection, creates $N$ branches, executes the block, and applies a universal `reconcile` rule to gather the results into a new structural collection.
* **Syntax Example**:
    ```ictl
    // 'batch_jobs' is an array of 5 elements.
    // This creates 5 parallel timelines simultaneously.
    let results = split_map job consume batch_jobs {
        let processed = heavy_compute(job) // Executes in isolated arena
        yield processed
    } reconcile (gather) // Combines yielded values into a new array
    ```

---

## Detailed Proposal: The "Perfect" ICTL Iteration

| Feature | Standard `for` | ICTL Entropic `for` |
| :--- | :--- | :--- |
| **Collection State** | Unchanged | **Explicitly Consumed or Cloned** |
| **Iteration Timing** | Variable (Jitter) | **Paced or Deterministically Bounded** |
| **Concurrency** | Sequential only | **Native `split_map` timeline generation** |
| **Execution Cost** | Implicit | **Analyzed at compile-time (WCET constraint)** |

### Comprehensive Syntax Example

```ictl
@0ms: {
    let sensor_data = [10, 25, 8, 42] // 4 elements
    
    // We want to process this destructively, with a maximum 
    // temporal fuel of 200ms total, pacing each step at 10ms.
    for reading consume sensor_data pacing 10ms (max 200ms) {
        if (reading > 20) {
            require System.Log(message="Threshold exceeded")
        } else {
            // Padding rule applies here as well
        }
        reconcile () // Internal branch reconciliation
    }
    
    // Static Analyzer Calculation:
    // 4 elements * 10ms pacing = 40ms total execution time.
    // 40ms < 200ms max fuel -> Compile PASS.
    // 'sensor_data' is now flagged as Consumed.
}
```

---

### Architectural Impact
This proposal solidifies ICTL's memory and time models. By forcing the developer to type `consume` or `clone` in the loop signature, you eliminate hidden memory overhead. By adding `pacing`, you turn loops into precise temporal metronomes, completely eliminating iteration drift