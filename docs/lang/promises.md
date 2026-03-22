# ICTL Promises (defer/await)

ICTL promises are a controlled asynchronous operation model built on top of entropic memory. They are designed for deterministic time semantics with explicit failure/timeout behavior.

## `defer` expression

Syntax:

```ictl
let x = defer System.NetworkFetch(url="api.data", latency="10") deadline 50ms
```

- `defer <capability>(...) deadline <N>ms` creates a `Pending` value in the local timeline arena.
- The expression stores a promise object; no payload is immediately available.
- `latency` and `deadline` are user-supplied parameters; actual wallclock behavior is simulated by the VM (`ready_at` based on current clock + latency).

### Semantics

1. On assignment, the target local variable enters `EntropicState::Pending`.
2. As with regular `let`, the variable is bound for later use.
3. `defer` cannot be used in a context that expects immediate `Valid` data without `await`.

## `await` statement

Syntax:

```ictl
await(x)
```

- `await` forces evaluation of a pending promise.
- If current local clock < `ready_at`, the branch advances clock to `ready_at` and consumes budget accordingly.
- If now <= `deadline_at`, promise resolves to `Valid(payload)`.
- If currently > `deadline_at`, promise transitions to `Consumed`.

### Behavior

- `await` on `Valid` is no-op.
- `await` on `Decayed` or `Consumed` is currently no-op (interpreted as an already finalized state).
- `await` must be used before consuming a pending variable via expressions like `print`, field access, or `match entropy` that dereference it.

## `match entropy` with promises

Promises introduce a new branch pattern:

```ictl
match entropy(x) {
  Pending(p): { ... }
  Valid(v): { ... }
  Decayed(d): { ... }
  Consumed: { ... }
}
```

- `Pending(p)` is taken when value is still unresolved.
- `Valid(v)` executes when promise fulfilled.
- `Consumed` handles expired or consumed promise values.

## Example

```ictl
@0ms: {
  let ds = defer System.NetworkFetch(url="api.slow", latency="100") deadline 20ms
  await(ds)
  match entropy(ds) {
    Pending(p): { let status = "pending" }
    Valid(v): { let status = v }
    Consumed: { let status = "timeout" }
  }
}
```

## Analyzer and VM notes

- Analyzer treats `defer` as an `Expression::Deferred` that is valid in any branch guardable by timeline consumption rules.
- `await` adds deterministic local clock advance (
  `max(current, ready_at)` plus budget) in VM and resolves/consumes pending state.
- `match entropy` now handles `Pending` branch specifically for unresolved deferrals.
