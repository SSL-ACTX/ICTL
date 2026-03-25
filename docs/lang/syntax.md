# ICTL Syntax Reference

This document provides a formal specification of the Isolate Concurrent Temporal Language (ICTL) syntax. ICTL is a research-focused language designed for deterministic, time-aware concurrency with entropic memory semantics.

---

## 1. Program Structure

An ICTL program consists of one or more **Timeline Blocks**.

### Timeline Blocks
A timeline block defines an execution context at a specific time coordinate.

**Syntax:**
```ictl
@<time_coordinate>: <statement_block>
```

**Time Coordinates:**
- **Absolute Time**: `@0ms:`, `@100ms:` (Time relative to the root timeline start).
- **Relative Time**: `@+10ms:` (Time offset relative to the current timeline entry).
- **Branch Identifier**: `@branch_name:` (Executes within a specific named branch).

**Example:**
```ictl
@0ms: {
  split main into [worker]
  @worker: {
    let x = "hello"
  }
}
```

---

## 2. Declarations and Expressions

### Variable Assignment (`let`)
Initializes a new binding in the current branch's entropic arena.

**Syntax:**
```ictl
let <identifier> = <expression>
```

**Entropic Impact:**
- If `<expression>` is a variable, it is **consumed** (moved) from its source and becomes unavailable unless wrapped in `clone()`.
- If `<expression>` is a literal, it is allocated in the local arena.

### Expression Types
- **Literals**:
  - String: `"hello"`
  - Integer: `42`, `-5`
  - Array: `[1, 2, 3]`
  - Struct: `struct { a = 1, b = "x" }`
- **Field Access**: `s.a` (Triggers **structural decay** in the parent struct `s`).
- **Clone Operation**: `clone(x)` (Creates a deep copy of `x`; consumes CPU budget based on payload weight).
- **Channel Receive**: `chan_recv(chan)` (Destructively reads a value from a channel).
- **Deferred Promise**: `defer <capability>(<params>) deadline <amount>ms` (Creates a `Pending` state value).

---

## 3. Control Flow

### Conditional Execution (`if`)
Performs speculative path evaluation followed by deterministic state reconciliation.

**Syntax:**
```ictl
if (<expression>) <statement_block> [else <statement_block>] [reconcile (<resolution_rules>)]
```

**Semantics:**
- Both branches are speculatively analyzed for entropic consistency.
- `reconcile` rules define how variables consumed in only one path are resolved.

### Speculation (`speculate`)
Creates a transient micro-timeline for trial computations with zero-leakage rollback.

**Syntax:**
```ictl
speculate (max <amount>ms) {
    <statements>
    commit { <statements> }
} [fallback { <statements> }]
```

**Semantics:**
- **Rollback**: On failure (`collapse` or timeout), the state is restored exactly to pre-speculation.
- **Commit**: On success, explicitly tagged variables (Selective mode) or the full state (Full mode) merge into the parent.

### Paced Iteration (`for`)
Iterates over collections with strict temporal and entropic constraints.

**Syntax:**
```ictl
for <item> <mode> <source> [pacing <amount>ms] [(max <amount>ms)] { <statements> }
```

**Modes:**
- `consume`: Source is destroyed; items are moved into the loop scope.
- `clone`: Source remains valid; items are cloned into the loop scope.

**Pacing:**
- Ensures each iteration takes exactly `Nms`. Overruns trigger a `WatchdogBite`.

### Parallel Mapping (`split_map`)
A scatter-gather construct spawning independent timelines for each element in a collection.

**Syntax:**
```ictl
split_map <item> <mode> <source> { <statements> } reconcile (<resolution_rules>)
```

---

## 4. Routines and Contracts

### Routine Definition (`routine`)
Defines a temporal procedure with a deterministic execution contract.

**Syntax:**
```ictl
routine <name>(<params>) taking (<amount>ms | _) { <statements> }
```

**Parameter Modes:**
- `consume`: Argument is moved into the routine.
- `clone`: Argument is copied.
- `peek`: Read-only access; caller state unchanged.
- `decay`: Caller value becomes `Decayed` after the call.

---

## 5. Timeline Management

### Branching (`split` / `merge`)
- **`split <parent> into [<branches>]`**: Spawns isolated child timelines.
- **`merge [<branches>] into <target> [resolving (<rules>)]`**: Recombines branch states.

### Resets and Anchors
- **`anchor <name>`**: Snapshots the current timeline state (clock and arena).
- **`rewind_to(<name>)`**: Restores the timeline to the specified anchor.
- **`watchdog <target> timeout <amount>ms [recovery <block>]`**: Monitors a branch and executes recovery logic on timeout.

---

## 6. Channels and Concurrency

### Communication
- **`open_chan <name>(<capacity>)`**: Initializes a buffered communication channel.
- **`chan_send <chan>(<value>)`**: Moves `<value>` into the channel buffer.
- **`chan_recv(<chan>)`**: Extracts a value from the channel.

### Isochronous Slicing
- **`slice <amount>ms`**: Sets a fixed-time execution slice for the current isolate.
- **`loop tick { <statements> }`**: Executes logic within a single slice, padding remaining time and committing channel buffers.

---

## 7. Diagnostics and Capabilities

### Observables
- **`print(<expression>)`**: Consuming output (standard entropic evaluation).
- **`debug(<expression>)`** / **`log(<expression>)`**: Non-consuming peek for inspection.

### Capability Manifests (`isolate`)
Sandboxes a block of code with specific resource requirements and capabilities.

**Syntax:**
```ictl
isolate [<identifier>] {
    [enable <resource>(<amount>)]
    [require <capability>(<params>)]
    [slice <amount>ms]
    <statements>
}
```

---

## 8. Low-Level / System Statements
- **`network_request <url>`**: Triggers a simulated network effect (cost: 5ms).
- **`reset <branch> to <anchor>`**: Implementation-level acausal reset.
- **`collapse`**: Immediately aborts the current speculative block.
