# RFC: Isolated Capability Timeline Language (ICTL) Specification

## 1. Abstract

The Isolated Capability Timeline Language (ICTL) is a deterministic, sandbox-oriented programming language engineered around explicit temporal execution, rigorous capability-based permissions, and foundational isolation. To enforce absolute memory safety without the utilization of traditional garbage collection or borrow checking mechanisms, ICTL implements an **Entropic Memory Model** (incorporating destructive reads by default) and **Deterministic Temporal Execution** (utilizing explicit timeline branching with relativistic local temporal coordinates). 

ICTL eliminates implicit behavioral patterns, shared mutable state, unrestricted resource access, and conventional asynchronous race conditions. It introduces **Acausal Anchors** for state restoration, governed by **Commit Horizons** to manage external side-effects securely. ICTL is designed for high-assurance systems, the execution of untrusted code, and secure distributed task orchestration.

---

## 2. Design Objectives

### 2.1 Security by Construction
- Absence of implicit permissions and ambient authority.
- Capabilities are declared via a static manifest at the initialization of an isolation block and are subject to strict policy constraints (e.g., temporal rate limits and domain restrictions).

### 2.2 Entropic Isolation (Structural Decay)
- **Destructive Reads**: The reading of a variable results in its consumption from the memory arena. 
- **Explicit Replication**: Data duplication requires explicit instructions and consumes execution budget deterministically based on structural depth and data volume.
- This model eliminates use-after-free vulnerabilities, double-free errors, and unintended data leakage at the Virtual Machine level.

### 2.3 Deterministic Temporal Execution and Concurrency
- Traditional threading models are replaced by explicit timeline `split` and `merge` operations.
- **Relativistic Temporal Logic**: Branched timelines operate via localized temporal clocks. Global temporal coordination is achieved exclusively at formal synchronization points (`merge`).
- **Deterministic Reconciliation**: State collisions during reconciliation result in static analysis errors unless a formal resolution strategy is specified.

### 2.4 Resource Governance and Causal Containment
- CPU, memory, and I/O resources must be explicitly authorized per isolate.
- Strict resource boundaries are enforced by the Virtual Machine.
- **Branch Termination**: If an execution branch exceeds its allocated resource budget or encounters a runtime fault, it is immediately terminated, its memory arena is reclaimed, and no side-effects are propagated.

---

## 3. Foundational Concepts

### 3.1 Timeline Execution and Relativistic Clocks
Programs are structured as time-indexed execution blocks. Time is utilized as the primary control flow mechanism. Upon a timeline split, child branches operate on a **Relative Local Clock** initialized at `@+0ms`. 

**Temporal Resolution Regulation**:
All temporal literals must resolve to a fixed minimum unit. Relative clocks are normalized to the global temporal coordinate during merge operations utilizing deterministic rounding protocols defined by the Virtual Machine.

```ictl
@0ms: execute main_sequence
@50ms: split main into [data_retrieval, watchdog_timer]
```

### 3.2 Parameterized Isolation Manifests
Computation occurs within `isolate` blocks. Capabilities are declared via a manifest at the block's initiation. To enforce granular security, capabilities incorporate strict policy parameters.

```ictl
isolate NetworkWorker {
    enable cpu(10ms)
    enable memory(1MB)
    
    // Parameterized Functional Capabilities
    require Net.Outbound(rate=5/s, domains=["api.internal.ictl"])
    require System.Entropy(mode="deterministic", seed=0x1A4)
}
```
*Note on Entropy Implementation*: Non-deterministic entropy (e.g., `mode="chaos"`) designates the timeline as non-replayable, suspending certain temporal debugging invariants.

### 3.3 Entropic Memory and Structural Decay Regulations
To ensure isolation, ICTL utilizes destructive reads. A "read" is defined as any operation that transfers ownership of a value.

- **Total Consumption**: Passing a variable to a routine, transmitting it via a channel, or performing a comprehensive pattern match consumes the entire object.
- **Structural Decay**: Accessing a specific field within a structure consumes only that field. The parent structure remains within the arena but is transitioned to the `Decayed` state. Subsequent attempts to access the parent structure or the consumed field result in static analysis errors.

**Replication Cost Model**:
The `clone(x)` operation incurs a deterministic CPU budget penalty: `Cost = Base_Overhead + (Size_in_Bytes * C) + (Structural_Depth * K)`. If the cost exceeds the remaining isolate budget, a temporal fault is triggered.

**Single-Ownership Invariant**:
At any point during execution, a value must possess exactly one valid owner. Any operation that would result in multiple active references without an explicit `clone()` operation is prohibited.

### 3.4 Temporal Concurrency (Split, Merge, and Reconciliation)
Concurrency involves the formal bifurcation of the memory arena. 

**Merge Semantics**:
- Only explicitly yielded values persist following a merge.
- If multiple branches yield a value to a single target binding, a reconciliation conflict is triggered.
- Developers must specify a resolution protocol: `first_wins`, `priority(branch)`, or a custom reconciliation routine.
- If a branch fails to yield a value for a required binding, it is treated as yielding a `void` state.

```ictl
@100ms:
  split main into [service_a, service_b]

@200ms:
  // Reconciliation conflict unless a protocol is specified:
  merge [service_a, service_b] into main resolving (user_data = first_wins)
```

### 3.5 Acausal Anchors and Commit Horizons
ICTL utilizes the `anchor` primitive for state restoration. 

**Causal Scope**: Anchors are restricted to the active timeline branch and its descendant isolates. A child branch may not restore the state of its parent.
**The Commit Horizon**: Since an anchor facilitates memory state restoration, external side-effects (e.g., network transmissions) could result in causal paradoxes. To finalize external state, side-effects must be encapsulated within a `commit` block. Following the execution of a `commit`, state restoration past that point is prohibited.
**Irreversibility Invariant**:
Upon completion of a `commit` block, all preceding anchors within the active timeline branch are invalidated for restoration operations crossing the commit boundary.

```ictl
anchor initialization_point
isolate {
    let payload = serialize_data()
    commit {
        Net.send(payload) // This operation establishes a commit horizon
    }
    // Restoration to initialization_point is prohibited following the commit
    initialize_subsystem() 
}
```

### 3.6 Communication Semantics
Channels are governed by entropic rules. Message transmission is single-consumer by default. Accessing a message consumes it from the channel. Broadcasting requires an explicit fan-out construct that applies the replication cost model to the payload for each subscriber.

### 3.7 Determinism Classifications

ICTL operates within two primary modes:

- **Deterministic Mode (Default)**:
  - All operations are replayable.
  - Entropy is deterministically seeded.
  - Execution is fully reproducible across disparate environments.

- **Non-Deterministic Mode**:
  - Enabled via specialized capabilities.
  - Suspends state restoration invariants across affected timelines.
  - Designates the execution as non-reproducible within systemic metadata.
  
---

## 4. Syntax Specification

### 4.1 Program Architecture
```ebnf
program ::= timeline_block+
```

### 4.2 Timeline Block Specification
```ebnf
timeline_block ::= "@" (time_literal | relative_time) ":" statement_block
relative_time  ::= "+" time_literal
```

### 4.3 Isolate and Manifest Specifications
```ebnf
isolate_block ::= "isolate" identifier "{" manifest statement* "}"
manifest      ::= (resource_decl | require_decl)*
require_decl  ::= "require" capability_path ("(" param_list ")")?
```

### 4.4 Temporal Control Flow Primitives
```ebnf
branch_stmt ::= "split" identifier "into" "[" identifier_list "]"
merge_stmt  ::= "merge" "[" identifier_list "]" "into" identifier ("resolving" "(" resolution_rules ")")?
anchor_stmt ::= "anchor" identifier
commit_stmt ::= "commit" "{" statement* "}"
```

---

## 5. Execution Model Architecture

1. **Static Analysis Phase**: Validation of monotonic temporal ordering, structural decay paths, and parameterized capability manifests.
2. **Allocation Phase**: Initialization of isolated arenas subject to specified memory constraints.
3. **Execution Phase**: Progression through global temporal coordinates.
4. **Bifurcation**: Upon a `split`, the memory arena is bifurcated. Child branches execute utilizing relative local temporal clocks.
5. **Reconciliation**: Upon a `merge`, divergent branches are synchronized at the reconciliation coordinate, conflicts are resolved, and arenas are collapsed into the parent context.

**Failure Management and Containment**:
- **Capability Violation**: Static analysis failure.
- **Resource Exhaustion**: Immediate branch termination and arena reclamation. The parent timeline identifies the branch as yielding a `void` or failure state.
- **Temporal Fault**: A deterministic runtime failure triggered by resource budget exhaustion (e.g., CPU exhaustion from replication costs). Results in immediate branch termination and state reclamation.

---

##  ICTL Technical Implementation Example

This implementation demonstrates parameterized capabilities, relative temporal logic, replication budgeting, and deterministic reconciliation.

```ictl
@0ms:
  isolate system_initialization {
    enable cpu(10ms)
    enable memory(512KB)
    require System.Entropy(mode="deterministic", seed=0x42)
    
    let trace_id = System.generate_uuid()
    // Replication is required for transmission to multiple branches
    let trace_id_replica = clone(trace_id) 
    
    send(trace_id) to @api_request
    send(trace_id_replica) to @watchdog
  }

@10ms:
  anchor execution_baseline
  split main into [api_request, watchdog]

// Branch 1: External API Request
@api_request:
  @+0ms: // Local temporal coordinate relative to the split
    isolate {
      enable cpu(50ms)
      enable memory(2MB)
      require Net.Outbound(rate=1/s, domains=["api.internal.ictl"])

      let id = receive() from main
      let payload = format("REQUEST_IDENTIFIER: {}", id) // 'id' is consumed
      
      commit {
        Net.send(payload) // 'payload' is consumed, side-effect finalized
      }
      
      let response = Net.await_response()
      yield response as api_result
    }

// Branch 2: Relativistic Watchdog Protocol
@watchdog:
  @+60ms: // Relative to the split; triggers if api_request duration > 60ms
    isolate {
      enable cpu(5ms)
      require System.Control
      
      let id = receive() from main // Consumption of the replicated ID
      System.Log(message="Temporal timeout for ID: " + id)
      
      // Localized state restoration, effectively cancelling the API wait
      System.rewind_to(execution_baseline)
      yield "TIMEOUT_STATE" as api_result
    }

@100ms: // Global temporal synchronization point
  // Explicit reconciliation required for 'api_result'
  merge [api_request, watchdog] into main resolving (api_result = first_wins)
  
  isolate {
    require System.IO
    System.Log(message="Finalized State: " + api_result)
  }
```
