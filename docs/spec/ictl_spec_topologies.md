# Entropic Topologies and Relativistic Memory

Entropic Topologies facilitate the implementation of complex, cyclically-related data structures and advanced index-based access while maintaining the Isolate Concurrent Temporal Language (ICTL) deterministic, zero-cost memory management model.

## Topological Literals

A `topology` is a collection of entropic states addressable by dynamic keys (string or integer identifiers).

```ictl
let network = topology {
    nodeA = { status = "active", load = 10 },
    nodeB = { status = "active", load = 20 }
}
```

## Index-Based Access

Topologies and structures support dynamic index access via the `[]` operator.

```ictl
let key = "nodeA"
let node = network[key]
```

## Entropic Entanglement Synchronization

Topologies may be entangled across parallel execution timelines. This ensures that any destructive operation—such as consumption or decay—performed on a topology field within one branch is immediately reflected across all entangled branches.

```ictl
@0ms: {
    let t = topology { first = 1, second = 2 }
    entangle(t)
    
    split main into [w1, w2]
    
    @w1: {
        let val = t["first"] // Triggers decay of 't' and consumption of 'first'
    }
    
    @w2: {
        match entropy(t["first"]) {
            Consumed: System.Log(message="Field 'first' consumed in parallel timeline.")
            Valid(v): System.Log(message="Field 'first' maintains Validity.")
        }
    }
}
```

## Structural Decay and Entropic Pattern Matching

The extraction of a field from a topology or structure transitions the parent container to the `Decayed` state. This state is managed via the `match entropy` construct.

```ictl
match entropy(my_topology) {
    Valid(t): 
        System.Log(message="Topology maintains structural integrity.")
    Decayed(d):
        System.Log(message="Topology has undergone partial consumption.")
        // 'd' represents the residual fragments
    Consumed:
        System.Log(message="Topology state is terminal.")
}
```

## Relativistic Key Management and Topographical Reconciliation

### 1. Relativistic Key Semantics
Within the ICTL framework, a topology key (e.g., `"node_1"`) is defined as a **Relativistic Key** rather than a direct physical pointer. It designates a semantic slot within the local timeline's memory arena.
- A `split` operation replicates the causal arena state into independent child branches.
- Each branch maintains a localized physical value for the designated key.
- Keys may diverge into distinct states across branches (e.g., `Valid` in one branch and `Consumed` in another).

### 2. Analysis of Relativistic Divergence
```ictl
@0ms: {
  let graph = topology {
    core = struct { status = "stable", backup = "node_b" },
    node_b = struct { status = "standby", backup = null }
  }

  split main into [worker_alpha, worker_beta]

  @worker_alpha: {
    let c = graph["core"]
    c.status = "upgrading"
  }

  @worker_beta: {
    let dead_core = consume(graph["core"])
  }
}
```
- In `worker_alpha`, `graph["core"]` reflects the `upgrading` status.
- In `worker_beta`, `graph["core"]` has been consumed, creating an entropic vacancy.

### 3. Topographical Reconciliation (Wavefunction Collapse)
```ictl
@10ms: {
  merge [worker_alpha, worker_beta] into main resolving (
    graph: topology_union {
      "core": priority(worker_alpha),
      _: decay
    }
  )
}
```
- `topology_union`: Facilitates the union of keys with localized resolution protocols.
- `topology_intersect`: Requires that all contributing branches maintain valid key values for successful resolution.

### 4. `on_invalid` Causal Reversion Protocol
The `topology_union` and `topology_intersect` mechanisms support an optional `on_invalid` clause for automated causal recovery:

```ictl
merge [alpha, beta] into main resolving (
  graph: topology_union {
    "core": priority(alpha),
    _ : decay,
    on_invalid: rewind alpha to base
  }
)
```

- The `on_invalid` clause is evaluated when the reconciliation outcome is semantically invalid due to entropic conflicts.
- `rewind <branch> to <anchor>` triggers a rollback of the specified branch to the designated anchor, followed by a re-execution with a consistent causality prefix.

## Implementation Architecture

The Stack-based Temporal Virtual Machine (STVM) utilizes a deterministic arena-based memory model:
1. **Consumption**: A `Valid` state transitions to `Consumed` upon movement.
2. **Decay**: A `Valid` structure or topology transitions to `Decayed` when a constituent field is extracted. 
3. **State Propagation**: Entanglement facilitates asynchronous state updates across parallel timelines, ensuring that temporal dependencies are reconciled during the merge phase.
