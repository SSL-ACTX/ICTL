# ICTL Proposal: Entropic Garbage Collection (EGC)

This document outlines the formal requirements for memory reclamation within the ICTL Virtual Machine. It introduces **Zero-Jitter Deallocation**, leveraging the Static Analyzer's entropic tracking to physically free host memory without the non-deterministic pauses of traditional Garbage Collection (GC).



## 1. The Problem with Traditional Memory Management
In traditional systems, memory is managed in one of three ways, all of which are fatal to ICTL’s temporal determinism:
* **Tracing GC (Mark-and-Sweep):** "Stop-the-world" pauses introduce massive, unpredictable temporal jitter.
* **Reference Counting (ARC):** Cascading decrements can cause a single variable drop to arbitrarily delay the `local_clock`.
* **Manual (`malloc`/`free`):** Highly unsafe, leading to use-after-free errors and memory leaks.

## 2. The EGC Philosophy: Entropy = Lifetime
In ICTL, the lifetime of a variable is mathematically tied to its entropic state. When a variable's entropy reaches absolute zero (i.e., it is `Consumed` across all possible timeline branches), its physical memory is immediately and deterministically freed.

Because the `EntropicAnalyzer` tracks every consume, split, and merge, EGC operates entirely without a runtime scanner. The compiler simply injects explicit `dealloc` instructions into the VM bytecode at the exact millisecond a value perishes.

## 3. The Three Pillars of EGC

### I. Static Deallocation Injection
When a variable is consumed in a standard block, the memory is reclaimed instantly. 
* **The Rule**: A `consume` operation (like sending to a channel or passing to a destructive routine) acts as an implicit, safe `free()`. The VM drops the payload from the local `Arena` at the exact temporal cost of 1ms.
* **Decay Handling**: If a struct undergoes `Structural Decay`, the parent container is logically destroyed, but the host memory remains alive until the final extracted field is consumed.

### II. Arena Bulk-Reclamation (The "Big Crunch")
When a micro-timeline or parallel branch ends, we don't need to trace its variables. The entire `Arena` is dropped at the OS level in $O(1)$ time.
* **`collapse` (Speculate)**: When a speculation fails, the temporary arena is instantly vaporized. 
* **`merge`**: When timelines reconcile, the un-yielded variables are immediately dropped. Only the variables explicitly named in the `reconcile` block are moved to the parent arena.

### III. Commit-Horizon Pruning (Acausal Memory)
The only complication in EGC is the `anchor` keyword. An anchor creates an acausal snapshot, meaning memory cannot be freed even if it is `Consumed` in the current timeline, because the VM might need to `reset` back to it.
* **The Rule**: Memory captured by an `anchor` enters a **Suspended Entropic State**.
* **The Horizon**: When execution passes a `commit { ... }` block, all prior anchors in that timeline are invalidated. At this exact millisecond, the VM bulk-drops the snapshots, reclaiming the host heap.

---

## 4. Under-the-Hood: EGC in Action

Here is how the developer's source code maps to the VM's implicit Garbage Collection operations.

### ICTL Source Code
```ictl
@0ms: {
    let large_dataset = NetworkRequest(domain="data.ictl") 
    
    speculate (max 50ms) {
        anchor parse_start
        
        // 'large_dataset' is consumed logically here
        let parsed = call process_data(consume large_dataset)
        
        if (parsed == "corrupt") {
            collapse // Aborts and restores 'large_dataset'
        }
        
        // Anchor is cleared. Memory for 'large_dataset' is permanently freed.
        commit (result = parsed) 
    } fallback {
        let result = "failed"
    }
}
```

### VM Execution Path (Invisible to Developer)
1. **`@0ms`**: `large_dataset` allocated in Main Arena.
2. **`speculate`**: Micro-Arena created.
3. **`anchor parse_start`**: Main Arena cloned (Suspended State).
4. **`call`**: `large_dataset` is passed. Inside the routine, it reaches zero entropy. However, because `parse_start` exists, the host memory is *not* freed yet.
5. **`commit`**: 
    * The Micro-Arena is merged. 
    * The `parse_start` anchor is destroyed. 
    * **[EGC TRIGGER]**: The VM detects `large_dataset` has zero entropy and no anchors. The payload is physically dropped from the host heap.

---

## 5. VM Implementation Impact
To implement Phase 14, the following architectural updates are required:
1.  **`src/memory.rs`**: Update `EntropicState` and `Arena` to leverage Rust's native `Drop` trait for $O(1)$ bulk reclamation.
2.  **`src/vm.rs`**: Ensure that `Statement::Commit` explicitly triggers a cleanup of all `AnchorPoint.arena_snapshot` data, reclaiming memory that was being held for acausal retries.
3.  **`src/analyzer.rs`**: Add a final validation pass ensuring no variable is left `Valid` or `Decayed` at the end of the `main` program block without being explicitly consumed, enforcing 100% memory safety.

---

### Architectural Impact
With EGC, you have completely eliminated the unpredictable latency spikes that plague languages like Go, Java, or C#. You get the memory safety of Rust, but with a concurrency model explicitly designed to drop memory in massive, predictable chunks at timeline boundaries.