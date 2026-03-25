# Entropic Garbage Collection (EGC)

This document describes the ICTL runtime strategy for deterministic memory reclamation, referred to as Entropic Garbage Collection (EGC).

---

## 1. Principles of EGC

Traditional garbage collection often introduces non-deterministic "stop-the-world" pauses. EGC achieves memory safety and efficiency through the following principles:

1. **Deterministic Timing**: Memory reclamation points are explicitly defined by language constructs and lifecycle events.
2. **Entropic State Tracking**: The VM tracks whether a value is `Valid`, `Decayed`, or `Consumed`. Memory is freed the moment a value enters the `Consumed` state.
3. **Branch-Local Arenas**: Each timeline has its own isolated arena. When a timeline terminates, its entire arena is reclaimed in a single O(1) operation.

---

## 2. Arena Management

Each branch maintains an `Arena` with a fixed `capacity` and a tracking counter for `used` bytes.

| Metric         | Description                                                           |
| :------------- | :-------------------------------------------------------------------- |
| **`capacity`** | The maximum memory allowed for the branch (configured via `isolate`). |
| **`used`**     | The current sum of weights of all `Valid` payloads in the arena.      |

### Weight Calculation
Payload weights are calculated deterministically:
- **Integer**: 8 bytes.
- **String**: Length of the string in bytes.
- **Struct**: Sum of field weights + 16 bytes overhead.
- **Array**: Sum of element weights + 16 bytes overhead.

---

## 3. Reclamation Lifecycle

### Continuous Reclamation
Memory is reclaimed immediately during:
- **Consumption**: `let x = y` frees the memory previously used by `y` and reallocates it to `x`.
- **Field Extraction**: Accessing `s.f` frees the weight of field `f` while keeping the rest of struct `s` (now `Decayed`).
- **Reassignment**: Overwriting an existing variable (`let x = ...`) frees the weight of the previous value.

### Lifecycle Events
- **Branch Termination**: When a branch is merged or explicitly terminated, the VM invokes `GarbageCollector::collect_branch`, clearing all associated bindings and snapshots.
- **Commit Horizon**: Executing `commit { ... }` marks a point of no return. The VM discards all `anchor` snapshots for that branch, freeing the memory used to store those historical states.

---

## 4. Manual Consolidation

The VM provides a `compact_consumed()` utility that can be manually or automatically triggered during `commit` or `merge` events. This removes `Consumed` markers from the arena's internal hash map, optimizing lookup performance for long-running timelines.

---

## 5. Temporal Cost of GC

Unlike traditional languages, GC in ICTL is not a separate "background" process. Memory operations (allocation and reclamation) are part of the statement execution cost. This ensures that memory management never introduces jitter or hidden costs into the `local_clock`.
