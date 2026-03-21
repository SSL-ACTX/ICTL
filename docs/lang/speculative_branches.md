# Speculative Branches (`speculate` / `fallback` / `collapse`)

This page documents the speculative control-flow construct in ICTL.

## 1. Motivation

- Traditional `if` is static speculation with reconcile rules.
- `speculate` allows transient trials with rollback and commit semantics, supporting zero-entropic leakage on failed paths.

## 2. Syntax

```ictl
@0ms: {
  speculation_mode(selective)
  let payload = "input"
  speculate (max 50ms) {
    let temp = payload
    let out = temp

    if (out == "bad") {
      collapse
    }

    commit {
      let final = out
    }
  } fallback {
    let final = "default"
  }
}

@60ms: {
  speculation_mode(full)
  let payload = "input"
  speculate (max 50ms) {
    let temp = payload
    let out = temp

    commit {
      let final = out
    }
  } fallback {
    let final = "default"
  }
}
```

- `speculation_mode(selective|full)`: sets the speculative commit mode for subsequent `speculate` blocks (default `selective`).
- `speculate (max Nm)`: creates a micro-timeline.
- `collapse`: abort speculative body and trigger fallback.
- `commit { ... }`: makes successful speculation visible to parent.
- `fallback { ... }`: executes only when speculation aborts/timeouts.

## 3. Semantic rules

- Micro timeline starts with parent arena snapshot.
- `local_clock` inside speculation is limited by `max`.
- On success, parent state may integrate speculative state based on commit mode:
  - `Selective` (default): only vars declared inside `commit` are merged.
  - `Full`: all speculative arena contents are merged.
- On failure, parent is restored exactly (stateless rollback), then fallback runs.
- `watchdog` handles timeout, where `max` overrun is treated as `collapse`.

## 4. VM configuration

In Rust VM API:

```rust
vm.set_speculative_commit_mode(ictl::runtime::vm::SpeculationCommitMode::Selective);
vm.set_speculative_commit_mode(ictl::runtime::vm::SpeculationCommitMode::Full);
```

## 5. Testing patterns

- Use integration tests in `tests/integration.rs`.
- Validate both `Full` and `Selective` modes.
- Ensure:
  - rollback preserves parent values on collapse.
  - time padding is exact (`max + fallback` semantics).
