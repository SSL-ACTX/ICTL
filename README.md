 <div align="center">

![ICTL Banner](https://capsule-render.vercel.app/api?type=waving&color=0:0ea5e9,100:0284c7&height=220&section=header&text=ICTL&fontSize=80&fontColor=ffffff&animation=fadeIn&fontAlignY=35&desc=Isolate%20Concurrent%20Temporal%20Language&descSize=20&descAlignY=55)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg?style=for-the-badge)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/Architecture-Temporal_VM-magenta.svg?style=for-the-badge)]()
[![Status](https://img.shields.io/badge/Status-Research-purple.svg?style=for-the-badge)](https://github.com/SSL-ACTX/ictl)

[**Core Concepts**](#core-concepts) &middot; [**Report Bug**](https://github.com/SSL-ACTX/ictl/issues) &middot; [**Getting Started**](#getting-started)

</div>

---

**ICTL** is a research-focused, Rust-based language and runtime for deterministic, time-aware concurrency. Instead of threads and shared mutable state, ICTL models computation as isolated timelines, entropic memory, and explicit temporal effects.

> [!NOTE]
> **Key Architecture**
> Parallel timelines (`split`/`merge`), structural decay (entropic moves), acausal interventions (anchors & watchdogs), and deterministic temporal costs for all operations.

## Why ICTL?

* **Deterministic Concurrency:** Minimize nondeterminism by isolating timeline arenas and requiring explicit merge logic.
* **Resource-Aware Semantics:** Memory and values are entropic first-class primitives (moves and consumption are explicit).
* **Temporal Debugging Model:** Every instruction advances local clocks; time-bound monitors and anchors enable robust self-healing patterns.

---

## Core Concepts

* **Entropic Memory:** Values are moved or consumed. Field access can break the parent's seal, preventing unsafe cross-timeline movement.
* **Timelines & Clocks:** Split timelines branch into isolated arenas. Each instruction advances a local clock deterministically.
* **Channels:** Destructive message passing across timelines utilizing `open_chan`, `chan_send`, and `chan_recv`.
* **Watchdogs & Anchors:** Monitors can reset failing branches to prior anchors (acausal resets) to dynamically recover temporal computations.
* **Paced Iteration (`for` + `split_map`):** New iteration primitives support explicit entropic collection semantics, deterministic timing via `pacing`, and scatter-gather parallel execution with `split_map`.
* **Conditional Branching (`if` / `else`):** Explicit path reconciliation is required when branches consume shared values.
* **Speculative Branching (`speculate` / `fallback` / `collapse`):** Optimize safe trial computations with rollback and explicit commit controls; can be configured via `speculation_mode(selective|full)`.
* **Routine Contracts (`routine` / `call`):** Temporal contract procedures with `taking Nms` and entropic parameter modes (`consume`, `clone`, `decay`, `peek`).

> See [docs/docs.md](docs/docs.md) for language docs and pages (iteration, split_map, if/else, speculations, routine contracts, syntax, semantics).

---

## Quick Example

The following example demonstrates a worker timeline with an anchor and a watchdog that intervenes if the worker exceeds a strict time budget.

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
````

-----

## Repository Layout

| Directory | Description |
| :--- | :--- |
| `src/frontend/` | Parser, AST types, and `ictl.pest` grammar. |
| `src/analysis/` | Static analyzer (entropic checks, capability manifests). |
| `src/runtime/` | Virtual Machine, memory model, and garbage collector. |
| `src/main.rs` | CLI entrypoint (`--check` / `--run`). |
| `examples/` | Sample `.ictl` programs used by integration tests. |
| `tests/` | Integration test suite. |

-----

## Getting Started

> [\!IMPORTANT]
> **Prerequisites**
> You must have the Rust toolchain installed (Rust 1.70+ is recommended).

Build and run the CLI locally using the following commands:

```bash
git clone https://github.com/SSL-ACTX/ictl.git
cd ictl

# Build the project
cargo build

# Run semantic check only
cargo run -- --check examples/sample.ictl

# Run (analyze + execute)
cargo run -- --run examples/sample.ictl
```

### CLI Flags

  * `--check` : Execute parse and semantic analysis only.
  * `--run` : Analyze and then execute in the VM (default behavior when omitted).

-----

## Testing

Run the integration test suite located in `tests/` (exercised by CI):

```bash
cargo test -- --nocapture
```

-----

## Development Notes

> [\!TIP]
> **Diagnostics and Capabilities**
>
>   * The analyzer produces statement-level diagnostics with source spans. Use these diagnostics to quickly locate capability or causality violations.
>   * Capability manifests (required capabilities for isolates) are enforced statically. To add a runtime handler, register it in `src/runtime/vm.rs` when initializing run-mode.

  * Garbage Collection (GC) runs on branch termination or merge events to reclaim branch arenas. Heuristics can be tuned within `src/runtime/gc.rs` if necessary.

-----

## Contributing

Contributions to the language design, standard library, and runtime are welcome.

1.  Open an issue describing the proposed design change or bug.
2.  Add focused unit or integration tests under `tests/` or adjacent to the modified modules.
3.  Follow the coding conventions outlined in `.github/copilot-instructions.md` and ensure you run `cargo fmt` before submitting a pull request.

-----

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
