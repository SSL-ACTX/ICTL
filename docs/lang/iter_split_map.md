# ICTL Paced iteration + split_map

This page details the iterative constructs in ICTL and their entropic, temporal semantics.

## `for` loop

### Syntax

```ictl
for x consume arr pacing 10ms (max 40ms) {
  let y = x
  // body cost <= 10ms
}
```

or

```ictl
for x clone arr pacing 5ms {
  let y = clone(x)
}
```

### Semantics

- Source consumption:
  - `consume`: `arr` is consumed at loop entry; object becomes consumed and cannot be used again.
  - `clone`: `arr` remains valid; each element is copied at clone-time cost.
- Iteration pacing:
  - The body runtime cost is measured in `local_clock` increments.
  - If body cost > `pacing`, runtime returns `PacingViolation`.
  - Pad with `pacing - body_cost` to make each iteration exact timing.
- Loop budget `max`:
  - Indicates maximum total allowed time for the loop. Once loop ends, runtime pads up to `max`.
  - Completing before max includes deterministic padding.

### Example

```ictl
@0ms: {
  let numbers = [1,2,3,4]
  for n consume numbers pacing 10ms (max 40ms) {
    // each iteration is equalized to 10ms
    let doubled = n * 2
  }
}
```

- Execution: 4 iterations × 10ms = 40ms exactly. `numbers` is consumed and unavailable afterward.

---

## `split_map`

### Syntax

```ictl
@0ms: {
  let data = [1, 2, 3]
  split_map item consume data {
    // each branch has item bound to element
    yield item * 10
  } reconcile (result=first_wins)
}
```

### Semantics

- `split_map` consumes `data` by default.
- The runtime spawns one child timeline per element.
- Each child:
  - receives independent copy of parent state (branch snapshot)
  - executes body statements in isolation
  - may emit `yield <value>` into child-local buffer `yielded`
- After each child completes, the root timeline collects yielded values into `splitmap_results`.

### Reconcile

- `reconcile` defines conflict and merge resolution for shared names.
- `first_wins` gives precedence to the first branch that defines a conflict.
- Future semantics can implement additional resolve strategies.

### Result

- After the loop, parent environment has a new variable `splitmap_results` (array)
- Combined output for the example above: `[10,20,30]`.
