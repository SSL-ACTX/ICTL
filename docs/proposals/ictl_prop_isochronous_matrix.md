# Proposal: The Isochronous Matrix (Fixed-Slice Scheduling)

This document specifies the architecture for high-performance execution within the Isolate Concurrent Temporal Language (ICTL). By utilizing **Forced Fixed Slices (Ticks)**, the Virtual Machine (VM) eliminates runtime padding overhead and channel locking mechanisms, facilitating near-native execution speeds while maintaining absolute causal determinism.

## 1. Limitations of Continuous Temporal Accounting
Previous ICTL implementations utilized instruction-level temporal tracking. If Branch A required 4ms and Branch B required 7ms, the VM dynamically applied 3ms of temporal padding to Branch A. 
- **Computational Overhead**: The VM consumes CPU cycles for temporal calculations and dynamic padding operations.
- **Architectural Solution**: The introduction of the **Law of Rigid Slices**. Temporal progression is conceptualized as a discrete, synchronized metronome rather than a continuous flow.

## 2. Fixed-Slice Execution Protocol (`slice` and `loop tick`)
An execution isolate may declare a fixed CPU temporal slice. All branches within the isolate execute in synchronized, discrete steps designated as **Ticks**.

- **Execution Mechanics**: Upon the declaration of a `slice 10ms`, every parallel timeline is allocated exactly 10ms of physical CPU time. 
- **Static Verification**: The Entropic Analyzer calculates the Worst-Case Execution Time (WCET) of the `loop tick` block. If the logic exceeds the 10ms slice, a compile-time failure is triggered.
- **Hardware-Level Efficiency**: Since the compiler guarantees that the logic block fits within the slice, the VM omits runtime padding. The execution thread remains idle at the hardware level until the global tick boundary is reached.

## 3. Double-Buffered Entropic Channels (Lock-Free I/O)
In a continuous temporal model, communication channels require mutexes or atomic locks to prevent race conditions during concurrent transmission and reception. Within the Isochronous Matrix, locking mechanisms are eliminated.

- **Phase and Tick Separation**: 
  - All `chan_send` operations occur during Tick $N$, writing data to a localized shadow buffer.
  - All `chan_recv` operations occur during Tick $N+1$, reading exclusively from the primary buffer committed during the previous tick.
- **Performance Impact**: This architecture eliminates inter-timeline blocking. Execution branches operate within parallel CPU caches and perform shared memory commits only at the conclusion of the slice boundary.

## 4. Syntax Specification

```ictl
@0ms: {
  // Initialization of a high-performance isolate with a 5ms heartbeat (200Hz)
  isolate signal_processor {
    slice 5ms 
    
    // Initialization of a lock-free, double-buffered channel
    open_chan signal_bus(100)
    
    split main into [sensor_reader, strategy_evaluator]
    
    @sensor_reader: {
      // Periodic execution advancing exactly one iteration per global tick
      loop tick {
        let packet = defer System.NetworkStream(port=8080) deadline 1ms
        
        match entropy(packet) {
          Valid(p): chan_send signal_bus(p) // Written to localized shadow buffer
          Pending(_): System.Log(message="Packet resolution pending")
        }
      }
    }
    
    @strategy_evaluator: {
      loop tick {
        // Retrieves data transmitted during the PRECEDING tick with zero locking overhead
        let new_signal = chan_recv(signal_bus) 
        
        match entropy(new_signal) {
          Valid(s): {
            if (s.value > 100) {
              call execute_response(clone s) taking 2ms
            }
          }
          Consumed: {}
        }
      }
    }
  }
}
```

## 5. High-Performance Optimization Factors

1.  **Hardware Synchronization**: Fixed-slice scheduling aligns with OS thread scheduling, facilitating thread pinning to specific cores and execution in bulk synchronous parallel (BSP) mode.
2.  **O(1) Memory Reclamation**: Entropic Garbage Collection (EGC) is performed exclusively at the tick boundary. As all branches synchronize at the same temporal coordinate, the VM can reclaim dead memory in bulk without concurrent management overhead.
3.  **Parallel Architecture Compatibility**: Fixed-slice execution with lock-free channel boundaries aligns with the operational model of GPUs and SIMD architectures, facilitating potential future cross-compilation to compute shaders.

---

### Architectural Significance
By enforcing a fixed CPU temporal slice, ICTL transitions from an interpreted research language to a high-performance systems language suitable for robotics, real-time control systems, and high-frequency financial applications.
