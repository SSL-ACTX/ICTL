# ICTL Syntax Reference

This reference is written as a simple, approachable guide for both newcomers and contributors.
The docs link each language construct to the semantics implemented in runtime and analyzer.

## 1. Timeline blocks

A timeline block defines an execution context at a time coordinate.

- `@0ms: { ... }` run in main timeline at global time 0.
- `@+10ms: { ... }` relative time offset from the current timeline entry.
- `@branchName: { ... }` timeline-specific branch code (`split` creates branches).

### Example

```ictl
@0ms: {
  split main into [worker]
  @worker: {
    let x = "hello"
  }
}
```

## 2. Declarations and expressions

### Variable bind

- `let name = expression`

### Expression types

- String literal: `"hello"`
- Integer literal: `42` or `-5`
- Array literal: `[1,2,3]`, `["a","b"]`
- Struct literal: `struct { a = 1, b = "x" }`
- Field access: `s.a`, with entropic field signals.
- Clone operation: `clone(x)` (entropic clone, costs budget).
- Channel receive: `chan_recv(chan)`.

### Simple example

```ictl
@0ms: {
  let arr = [1,2,3]
  let a = 5
  let b = a + 1
  let label = "ok"
}
```

## 3. Control flow

### `speculate` / `fallback` / `collapse`

```ictl
speculation_mode(selective)
speculate (max 10ms) {
  // speculative operations (move/consume,
  // clones, channel ops, etc.)
  let x = "hello"
  commit {
    let out = x
  }
} fallback {
  let out = "default"
}
```

- `speculation_mode(selective|full)` sets commit behavior for later speculative blocks (default `selective`).
- `speculate` creates a micro-timeline and does not affect parent unless commit succeeds.
- On `collapse` (or timeout) the speculative arena is discarded and fallback executes.
- On commit success, one can run in either:
  - `Selective` (default): only explicit commit values move up
  - `Full`: entire speculative child state merges into parent (configured with VM option).

### `if` statement

```ictl
if (a == b) {
  let result = "match"
} else {
  let result = "no"
} reconcile (x=first_wins)
```

- if/else body paths are speculatively analyzed for entropic consistency.
- `reconcile` rules are required when cross-path consumption affects persistency.

### `loop`

```ictl
loop (max 100ms) {
  let x = "step"
  break
}
```

- Recalculates local clock until max reached.
- `break` exits; runtime puts padding to fill `max`.

### `for` iteration (paced)

```ictl
for item consume arr pacing 10ms (max 40ms) {
  let value = item
}
```

- `consume`: array is destructively read.
- `clone`: array remains, clone costs are consumed.
- `pacing`: each iteration takes exactly Nms (body + padding).
- `max`: enforces total timeline budget when loop completes.

### `split_map`

Parallel scatter-gather variant:

```ictl
@0ms: {
  let data = [1,2,3]
  split_map item consume data {
    yield item
  } reconcile (result=first_wins)
}
```

- One child branch per element.
- Each child may `yield` values.
- Root collects values into `splitmap_results`.

## 4. Timeline management

- `split parent into [a,b]` creates branches `a`, `b`.
- `merge [a,b] into main resolving(v=name)` merges branch values.
- `anchor name` snapshots a branch.
- `rewind_to(name)` returns branch to anchor state (chaos mode blocks).
- `watchdog target timeout Nms recovery { ... }` monitors branches.

## 5. Channels

- `open_chan ch(10)` opens buffered channel.
- `chan_send ch(x)` sends destructively.
- `chan_recv(ch)` receives from channel.

## 6. Operators and precedence

- Arithmetic: `+`, `-`, `*`, `/`
- Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
- Including `for` pacing and loop values, these form the core runtime semantics.

## 7. Developer tips

- Run `cargo test -- --nocapture` for full integration checks after syntax changes.
- Use `analyzer` to validate entropic path correctness before executing VM code.
- Keep `if/else` `reconcile` rules explicit for deterministic merge semantics.
