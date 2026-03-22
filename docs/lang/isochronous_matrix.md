# Isochronous Matrix (Slice + Tick) Semantics

This feature is an extension of the ICTL temporal model for high-performance deterministic loops.

## Goal

Enable fixed-slice scheduling inside an isolate and enforce per-tick worst-case timing at analysis time.

## New constructs

- `slice Nms`: defines a fixed tick window in a surrounding isolate manifest.
- `loop tick { ... }`: executes once per tick, with local clock padding and channel-phase commit.

## Syntax

```ictl
@0ms: {
  isolate fast {
    slice 5ms
    open_chan c(8)

    // tick N: produce
    loop tick {
      let v = 10
      chan_send c(v)
      break
    }

    // tick N+1: consume previous tick value
    loop tick {
      let x = chan_recv(c)
      print(x)
      break
    }
  }
}
```

## Analyzer rules

- `slice` in a manifest binds `cpu_budget_ms` and `slice_ms` in timeline state.
- `loop tick` without any active `slice` is invalid (raises `TickLoopWithoutSlice`).
- `loop tick` static cost must not exceed `slice_ms` (`TickLoopBudgetExceeded`).

## Runtime behavior

1. `loop tick` body executes until `break`, then any remaining `slice_ms` is padded by increasing local clock.
2. Channel send inside tick writes to `pending_channels` (double-buffered path).
3. At tick end, all `pending_channels` are appended into live `channels` (phase-handshake).
4. `chan_recv` reads from `channels` current committed contents (previous tick payloads).

## Why this is useful

- reduces per-statement micro clock bias, reduces runtime branch and clock arithmetic overhead.
- supports deterministic bulk-synchronous pipeline: production and consumption are distinct phases.
- allows the VM to accept a single static timing limit per tick.
