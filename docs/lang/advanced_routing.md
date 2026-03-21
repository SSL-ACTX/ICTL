# Advanced Routing: `select` and `match entropy`

This page documents the advanced routing control flow constructs in ICTL.

## 1. `select` (Temporal Multiplexing)

`select` is a deterministic channel race and timeout macro.

### Syntax

```ictl
@0ms: {
  open_chan c(1)
  let msg = "ping"
  chan_send c(msg)

  select (max 10ms) {
    case data = chan_recv(c):
      let out = data
    timeout:
      let out = "timeout"
  } reconcile (out=first_wins)
}
```

### Rules

- `select (max Nms)` creates a bounded waiting period.
- `case <name> = chan_recv(<chan>)` only executes when the channel has data.
- `timeout:` executes if no case is ready before `max`.
- The final local branch is reconciled by `reconcile(...)`.
- The total cost is fixed to `1 + max(case_wcet, timeout_wcet)` (including base overhead) with explicit padding.

## 3. `match entropy` (State-Based Routing)

`match entropy(x)` routes based on `x`’s entropic state (valid, decayed, consumed).

### Syntax

```ictl
@0ms: {
  let user = struct { id = "1", name = "Alice" }

  match entropy(user) {
    Valid(u):
      let result = u.id
    Decayed(u):
      let result = "decayed"
    Consumed:
      let result = "missing"
  }
}
```

### Rules

- `Valid(u)` runs when `user` is fully intact and may be moved.
- `Decayed(u)` runs when fields were consumed but struct exists (partial seal).
- `Consumed` runs when `user` is already consumed.
- `Valid/Decayed` introduce local binding for payload usage.
- No extra reconciliation required; the `match entropy` handler itself is deterministic.

## 3. Examples

- `select` ensures deterministic runtime and supports prioritized channel selection.
- `match entropy` provides safe decay-aware routing for entropic memory workflows.
