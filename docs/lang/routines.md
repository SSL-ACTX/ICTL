# Routine Temporal Contracts

This page documents `routine` + `call` support in ICTL.

## Syntax

```ictl
routine process_payment(consume auth_token, peek details) taking 25ms {
  let amount = details.amount
  yield amount
}

@0ms: {
  let token = "secure123"
  let tx = struct { amount = "100", currency = "USD" }
  let result = call process_payment(token, tx)
}
```

## Parameter modes

- `consume`: caller variable is moved and marked consumed in caller scope.
- `clone`: caller keeps value intact; routine gets read-only copy semantics.
- `decay`: caller value becomes `Decayed` after call.
- `peek`: read-only; caller state unchanged.

## Timing rules

- `routine ... taking Nms` is a worst-case execution contract.
- Runtime semantics:
  - if body costs < Nms, VM pads to Nms.
  - if body costs > Nms, VM raises `WatchdogBite`.
- Routine body cannot include `split`, `merge`, or explicit `@...` timeline blocks.

## Nested contract check

Static analyzer validates each routine body using a recursive call cost model:
- `call` expression cost is `callee taking_ms` plus argument expression cost.
- Routine cost is estimated as body path maximum (if conditional) and must fit taking.

## Yield return values

- `yield <expr>` inside routine collects one or more values in return path.
- `call` returns first `yield` value, or `void` when nothing yielded.
