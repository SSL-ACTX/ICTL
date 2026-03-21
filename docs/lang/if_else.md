# ICTL Conditional Branches (`if` / `else`)

## Branch semantics

`if` in ICTL is a deterministic, analyzer-driven conditional with entropic resource safety.

### Execute path

- At runtime, only the taken branch is executed.
- Analyzer still models both `then` and `else` paths to check entropy and value state.

### Entropic rules

- Values consumed in one branch and referenced afterward require reconciliation.
- Without a `reconcile` rule, the analyzer can reject with a `CrossPathConsume` type error.
- Branch-local bindings are scoped to that branch unless persisted by merge/reconcile.

## Reconciliation

- `reconcile (x=first_wins)`:
  - `x` from chosen branch wins (if present), otherwise fallback path.
- `reconcile (x=priority(if))` / `(x=priority(else))`:
  - explicit preference for conflict resolution.
- `reconcile (x=decay)`:
  - enforces x as consumed/invalid in merged state, preventing use-after-consume.

## Example 1: safe consume

```ictl
@0ms: {
  let x = "foo"
  if (1 == 1) {
    let y = x
  } else {
    let z = "bar"
  } reconcile (x=first_wins)
}
```

- Here `x` is owned by the `if` block path and protected by first-wins.

## Example 2: conflict detection

```ictl
@0ms: {
  let x = "foo"
  if (0 == 1) {
    let x = "a"
  } else {
    let x = "b"
  } reconcile (x=priority(else))
}
```

- `x` in final context resolves to `"b"` via explicit branch priority.
