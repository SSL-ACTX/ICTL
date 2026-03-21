# ICTL Runtime GC / EGC Behavior

This document describes the ICTL runtime garbage collection strategy, also known as Entropic Garbage Collection (EGC), and how the VM implements deterministic memory reclamation.

## Goals

- Preserve temporal determinism by avoiding non-deterministic runtime scanning (traditional GC).
- Maintain memory safety with entropic state tracking (Valid / Decayed / Consumed).
- Provide explicit, deterministic points for arena reclamation (branch end / commit horizon).
- Provide developer-facing semantics for `anchor`, `commit`, `rewind`, `split`, `merge`, `speculate`, and `send`.

## Key concepts

### Arena

Each timeline has an `Arena` representing the branch-local memory store.

- `Arena.bindings: HashMap<String, EntropicState>`
- `Arena.used`: bytes currently counted as allocated for `Valid` payloads
- `Arena.capacity`: max allowed bytes

`EntropicState` values:
- `Valid(payload)` - value is available and counted toward budget.
- `Decayed(fields)` - struct has been partially consumed into fields.
- `Consumed` - value was consumed and should not be read again.

### EGC pillars

1. **Static dealloc on consume and update**
   - `consume` immediately decrements memory weight from the arena.
   - `set_consumed` and `decay` follow entropic type guarantees.
   - `insert` overwrites and adjusts `Arena.used` by subtracting any prior valid value.

2. **Branch lifecycle cleanup**
   - `GarbageCollector::collect_branch` is invoked when a sub-timeline is terminated.
   - Clears anchor snapshots and resets the arena (in O(1)).

3. **Commit horizon pruning**
   - When a branch executes `commit`, all anchor snapshots are cleared.
   - `commit_horizon_passed` is set on the timeline.
   - Rewind past commit results in runtime error.

## Runtime semantics

### Anchor / Rewind

- `anchor <name>` creates a snapshot object with branch-local arena clone and local clock.
- `rewind_to(<name>)` moves timeline state back to anchor snapshot state unless commit horizon passed.
- In Chaos entropy mode, rewind is prohibited.

### Commit

- `commit { ... }` marks a stable checkpoint and discards anchor snapshots.
- `commit_horizon_passed = true` after commit.
- `GarbageCollector::collect_commit_anchors` clears all snapshots.

### Split / Merge

- `split` clones parent arena into child branches; each branch has independent lifecycle.
- `merge` reconciles variable states by rules;
- children are collected after successful merge by `GarbageCollector::collect_branch`.

### Speculation

- `speculate` executes with local branch snapshots and optional fallback.
- On success and commit semantics, selected commit variables propagate.
- On collapse or failure, child state is discarded.
- During speculation, anchors are ephemeral and can be cleared on commit.

## Version pointers

- This page reflects the P14 EGC proposal behavior from the repository state as of March 21, 2026.

## Example flow

```
@0ms: {
  split main into [worker]
}
@worker: {
  anchor start
  let x = "work"
  commit {
    let y = "done"
  }
  rewind_to(start)  # error: commit horizon reached
}
```

- Anchor stored at `start`.
- `commit` clears snapshots and marks horizon.
- `rewind_to(start)` fails with `CommitHorizonViolation`.

## Notes

- Because `Arena::insert` now supports safe overwrite, reassignments are memory-consistent.
- `compact_consumed()` is available as a manual consolidation utility for runtime paths where explicit GC-like cleanup is beneficial.
