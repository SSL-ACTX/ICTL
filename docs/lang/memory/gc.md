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
Payload weights are calculated with sub-kilobyte precision:
- **Integer / Null**: 8 bytes.
- **String**: Length + 24 bytes overhead (Rust struct + pointer).
- **Struct**: Sum of field weights + 48 bytes (HashMap overhead).
- **Topology**: Sum of field weights + 64 bytes.
- **Array**: Sum of element weights + 24 bytes (Vec overhead).

**Entropic State Overhead**:
- **Valid(p)**: `weight(p)` + 16 bytes.
- **Decayed(f)**: `weight(fields)` + 32 bytes.
- **Consumed**: 8 bytes (placeholder marker).

**Binding Overhead**:
Variable names (keys) themselves consume memory: `len(key)` + 32 bytes (HashMap node overhead).

---

## 3. Reclamation Lifecycle

### Continuous Reclamation
Memory is tracked at a granular level:
- **Consumption**: Assigning a value (`let x = y`) marks `y` as `Consumed`. Its memory is not fully freed until a `compact_consumed()` occurs; however, its footprint is reduced to 8 bytes.
- **Field Extraction**: Accessing `s.f` consumes `f` and transitions `s` to `Decayed`. The arena weight is updated to reflect the new state of the parent.
- **Reassignment**: Overwriting an existing variable reclaims the weight of the previous state and key before allocating the new one.

### Lifecycle Events
- **Branch Termination**: When a branch is merged or explicitly terminated, the VM invokes `GarbageCollector::collect_branch`, clearing all associated bindings and snapshots.
- **Commit Horizon**: Executing `commit { ... }` marks a point of no return. The VM discards all `anchor` snapshots for that branch, freeing the memory used to store those historical states.

---

## 4. Manual Consolidation

The VM provides a `compact_consumed()` utility that can be manually or automatically triggered during `commit` or `merge` events. This removes `Consumed` markers from the arena's internal hash map, optimizing lookup performance for long-running timelines.

---

## 5. Temporal Cost of GC

Unlike traditional languages, GC in ICTL is not a separate "background" process. Memory operations (allocation and reclamation) are part of the statement execution cost. This ensures that memory management never introduces jitter or hidden costs into the `local_clock`.
