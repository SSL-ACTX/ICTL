# ICTL Overview

ICTL is a temporal, branch-isolated concurrency language targeting deterministic, entropic semantics.

Key design pillars:

- explicit value consumption / decay (entropic memory) 
- timeline split/merge with deterministic local clocks
- explicit capability manifests and sandboxed timeline environment
- temporal control with watchdogs, anchors, and pacing
- iteration with `for` (`consume` / `clone`) and scatter/gather `split_map`
- fixed-slice isochronous scheduling with `slice` + `loop tick` (see `docs/lang/isochronous_matrix.md`)

