# Entropic Garbage Collection (EGC)

This document specifies the Isolate Concurrent Temporal Language (ICTL) runtime strategy for deterministic memory reclamation, designated as Entropic Garbage Collection (EGC).

---

## 1. Principles of EGC Architecture

Conventional garbage collection mechanisms frequently introduce non-deterministic execution pauses. EGC achieves memory safety and efficiency through the following foundational principles:

1. **Deterministic Temporal Scheduling**: Memory reclamation points are formally defined by language constructs and lifecycle transitions.
2. **Entropic State Monitoring**: The Stack-based Temporal Virtual Machine (STVM) monitors whether a value occupies the `Valid`, `Decayed`, or `Consumed` state. Memory is reclaimed precisely when a value enters the `Consumed` state.
3. **Branch-Localized Arenas**: Every execution timeline maintains an isolated memory arena. Upon timeline termination, the associated arena is reclaimed via a single O(1) operation.

---

## 2. Arena Management Specifications

Each branch maintains a memory `Arena` characterized by a fixed `capacity` and a tracking mechanism for `used` bytes.

| Metric Identifier | Functional Description                                                                |
| :---------------- | :------------------------------------------------------------------------------------ |
| **`capacity`**    | The maximum memory allocation permitted for the branch (specified via `isolate`).      |
| **`used`**        | The cumulative sum of the weights of all `Valid` payloads currently within the arena. |

### Payload Weight Calculation
Payload weights are calculated with sub-kilobyte precision:
- **Integer / Null**: 8 bytes.
- **String**: Length + 24 bytes (Rust structure and pointer overhead).
- **Structure**: Cumulative field weights + 48 bytes (HashMap overhead).
- **Topology**: Cumulative field weights + 64 bytes.
- **Array**: Cumulative element weights + 24 bytes (Vector overhead).

**Entropic State Metadata Overhead**:
- **Valid(p)**: `weight(p)` + 16 bytes.
- **Decayed(f)**: `weight(fields)` + 32 bytes.
- **Consumed**: 8 bytes (terminal state marker).

**Binding Overhead**:
Variable identifiers themselves contribute to memory consumption: `len(key)` + 32 bytes (HashMap node overhead).

---

## 3. Reclamation Lifecycle Protocols

### Granular Reclamation
Memory tracking occurs at a granular level:
- **Consumption**: The assignment of a value (e.g., `let x = y`) transitions `y` to the `Consumed` state. While full reclamation is deferred until a `compact_consumed()` operation, the footprint is immediately reduced to 8 bytes.
- **Structural Field Extraction**: Accessing `s.f` consumes `f` and transitions `s` to the `Decayed` state. The arena weight is updated to reflect the parent structure's modified state.
- **Variable Reassignment**: Overwriting an existing variable results in the reclamation of the previous state and identifier weights prior to the allocation of new resources.

### System Lifecycle Events
- **Branch Termination**: Upon the merge or explicit termination of a branch, the VM executes `GarbageCollector::collect_branch`, reclaiming all associated bindings and snapshots.
- **Commit Horizon**: The execution of `commit { ... }` establishes a point of no return. The VM discards all `anchor` snapshots for the branch, reclaiming the memory utilized for historical state preservation.

---

## 4. Manual State Consolidation

The STVM provides a `compact_consumed()` utility that may be invoked manually or triggered automatically during `commit` or `merge` events. This operation removes `Consumed` markers from the arena's internal mapping structures, optimizing lookup efficiency for long-duration timelines.

---

## 5. Temporal Cost of Memory Management

In contrast to traditional languages, garbage collection in ICTL is not an asynchronous background process. Memory operations—both allocation and reclamation—are integrated into the execution cost of individual statements. This ensures that memory management never introduces jitter or hidden temporal costs into the `local_clock`.
