<div align="center">

![ICTL Banner](https://svg-banners.vercel.app/api?type=luminance&text1=ICTL&text2=Isolate%20Concurrent%20Temporal%20Language&width=800&height=160&color=7DD3FC)

</div>

# ICTL — Isolate Concurrent Temporal Language

ICTL is a research-focused, Rust-based language and runtime for deterministic, time-aware concurrency.
Instead of threads and shared mutable state, ICTL models computation as isolated timelines, entropic memory, and explicit temporal effects.

This README gives a concise overview, usage examples, developer notes, and suggestions for a clearer project name (see "Rename suggestions").

--

**Key ideas:** Parallel timelines (`split`/`merge`), structural decay (entropic moves), acausal interventions (anchors & watchdogs), and deterministic temporal costs for operations.

## Why ICTL?

- Deterministic concurrency: minimize nondeterminism by isolating timeline arenas and requiring explicit merge logic.
- Resource-aware semantics: memory and values are entropic first-class primitives (moves/consumption are explicit).
- Temporal debugging model: every instruction advances local clocks; time-bound monitors and anchors enable self-healing patterns.

--

## Core Concepts (short)

- Entropic Memory: values are moved/consumed; field access can break the parent's seal, preventing unsafe cross-timeline movement.
- Timelines & Clocks: split timelines branch isolated arenas; each instruction advances a local clock deterministically.
- Channels: destructive message passing across timelines using `open_chan`, `chan_send`, and `chan_recv`.
- Watchdogs & Anchors: monitors can reset failing branches to prior anchors (acausal resets) to recover temporal computations.

--

## Quick Example

Demonstrates a worker timeline with an anchor and a watchdog that intervenes if the worker exceeds a time budget.

```ictl
@0ms: {
  split main into [worker]

  @worker: {
    anchor start_point
    let data = "initial"
    let step1 = do_work()
    let step2 = do_more()
  }
}

@10ms: {
  watchdog worker timeout 2ms recovery {
    require System.Log(message="worker exceeded budget")
    reset worker to start_point
    require System.Log(message="worker rewound")
  }
}
```

--

## Repository Layout

- `src/frontend/` — parser, AST types and `ictl.pest` grammar
- `src/analysis/` — static analyzer (entropic checks, capability manifests)
- `src/runtime/` — VM, memory model, garbage collector
- `src/main.rs` — CLI entrypoint (`--check` / `--run`)
- `examples/` — sample `.ictl` programs used by integration tests
- `tests/` — integration tests

--

## Getting Started (dev)

Prerequisites: Rust toolchain (Rust 1.70+ recommended)

Build and run the CLI locally:

```bash
git clone https://github.com/SSL-ACTX/ictl.git
cd ictl

# Build
cargo build

# Run semantic check only
cargo run -- --check examples/sample.ictl

# Run (analyze + execute)
cargo run -- --run examples/sample.ictl
```

CLI flags:

- `--check` : parse + semantic analysis only
- `--run` : analyze then execute in the VM (default when omitted)

--

## Testing

Run the test suite with:

```bash
cargo test -- --nocapture
```

Integration tests are in `tests/` and exercised by the CI.

--

## Development Notes

- The analyzer produces statement-level diagnostics with source spans. Use those diagnostics to quickly locate capability or causality violations.
- GC runs on branch termination / merge events and reclaims branch arenas; tune heuristics in `src/runtime/gc.rs` if needed.
- Capability manifests (required capabilities for isolates) are enforced statically. To add a runtime handler, register it in `src/runtime/vm.rs` when starting run-mode.

--

## Contributing

Contributions are welcome. Please:

1. Open an issue describing the design change or bug.
2. Add focused unit or integration tests under `tests/` or adjacent to modified modules.
3. Follow the coding conventions in `.github/copilot-instructions.md` and run `cargo fmt`.

--

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file.