
# ICTL Proposal: Relativistic I/O & Entropic Promises

This document outlines the formal requirements for handling external, non-deterministic input/output (I/O) within the ICTL Virtual Machine. It introduces the **Pending** entropic state and the concept of **Timeline Suspension** to safely interface with the outside world without compromising internal causality.

## 1. The I/O Paradox
In ICTL, every instruction costs a predictable amount of time. External I/O violates this. If a timeline issues an HTTP request, the VM cannot "pad" the execution because the worst-case execution time (WCET) is unbounded. 

Therefore, I/O cannot be evaluated synchronously. It must be decoupled from the `local_clock`.

## 2. The `Pending` Entropic State
To handle data that "will exist but doesn't yet," we expand the `EntropicState` enum.
* **`Valid(payload)`**: Data is fully materialized.
* **`Decayed(fields)`**: Data is partially consumed.
* **`Consumed`**: Data is gone.
* **`Pending(promise_id)`**: *[NEW]* Data is currently traversing the causality boundary from the outside world.

## 3. The `defer` Keyword and `deadline` Contract
External requests are initiated using `defer`. This immediately returns a variable in the `Pending` state, costing exactly 1ms of `local_clock`. The actual work happens outside the VM's temporal accounting.

Crucially, every `defer` must include a `deadline`. This acts as a hard boundary for the non-determinism.

```ictl
@0ms: {
    // Costs 1ms. 'user_data' is immediately created as Pending.
    // The VM schedules the host to perform the fetch in the background.
    let user_data = defer System.NetworkFetch(url="api.ictl/user/1") deadline 200ms
    
    // The timeline continues executing immediately!
    let local_calc = 50 * 20
}
```

## 4. Synchronization (`await` vs. `match entropy`)

A timeline cannot access the fields of a `Pending` variable. To actually use the data, the timeline must synchronize with the external event.

### Option A: Strict Synchronization (`await`)
`await` pauses the `local_clock` of the *current branch only*, effectively detaching it from the `global_clock` until the promise resolves or the deadline strikes. 
* If the data arrives, the `local_clock` jumps to match the elapsed time.
* If the `deadline` is breached, the `await` call returns a `Decayed` or `Consumed` state (representing a timeout).

```ictl
@worker: {
    let dataset = defer System.DBQuery(query="SELECT *") deadline 500ms
    
    // The branch stops here. If the DB takes 40ms, local_clock += 40.
    // If the DB takes 600ms, it aborts at 500ms.
    await(dataset) 
}
```

### Option B: Entropic Routing (Non-Blocking)
Instead of waiting, a timeline can use Phase 10's `match entropy` to check if the data has arrived yet, treating time as just another physical state of memory!

```ictl
@worker: {
    let payload = defer System.NetworkFetch(url="api.data") deadline 1000ms
    
    // Do other work for 50ms...
    
    match entropy(payload) {
        Pending(p):
            require System.Log(message="Data not here yet. Doing fallback logic.")
        Valid(data):
            // Data arrived incredibly fast! We can use it.
            let parsed = data.id
        Consumed:
            // The deadline passed and the request failed/timed out.
            require System.Log(message="Request timed out.")
    }
}
```

## 5. VM Implementation Rules
1.  **Host Bridge**: The `capability_handlers` in the VM must be updated to return Rust `Future`s or spawn background threads for `defer` calls, rather than blocking the main VM thread.
2.  **Clock Desync**: When a branch hits `await`, it enters a `Suspended` state. The VM's `global_clock` continues advancing other parallel branches. Once the I/O completes, the suspended branch is re-inserted into the active queue.

---

### Why this is the perfect conclusion to ICTL:
By treating asynchronous data as an **Entropic State** (`Pending`), you seamlessly integrate non-deterministic I/O into the exact same routing and memory safety rules you've already built. It turns network latency into a memory-management problem, which ICTL is perfectly designed to solve.