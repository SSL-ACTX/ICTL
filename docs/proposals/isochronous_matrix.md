# ICTL Proposal: The Isochronous Matrix (Fixed-Slice Scheduling)

This document outlines the architecture for extreme-speed execution in ICTL. By abandoning micro-temporal accounting in favor of **Forced Fixed Slices (Ticks)**, the VM eliminates runtime padding overhead, removes all channel locking, and unlocks near-native C/Rust speeds while maintaining 100% causal determinism.

## 1. The Bottleneck of Continuous Time
In earlier phases, ICTL tracked time at the instruction level. If Branch A took 4ms and Branch B took 7ms, the VM dynamically padded Branch A with 3ms of No-Ops. 
* **The Cost:** The VM spends CPU cycles doing "time math" and dynamic padding.
* **The Solution:** We introduce the **Law of Rigid Slices**. Time is no longer a fluid river; it is a metronome.

## 2. The Law of Rigid Slices (`slice` & `@tick`)
An isolate can now declare a fixed CPU time slice. Every branch within that isolate executes in rigid, locked steps called **Ticks**.

* **The Mechanics:** If you declare a `slice 10ms`, every parallel timeline is forcibly given exactly 10ms of real-world CPU time. 
* **Static Enforcement:** The Entropic Analyzer calculates the WCET of the `@tick` block. If the logic cannot fit inside the 10ms slice, it is a compile-time hard failure.
* **Zero Runtime Padding:** Because the compiler guarantees the block fits in the slice, the VM doesn't need to inject No-Ops. The thread simply sleeps natively at the hardware level until the global tick boundary is reached.

## 3. Double-Buffered Entropic Channels (Lock-Free I/O)
In a continuous time model, channels require mutexes or atomic locks to prevent race conditions when Branch A sends and Branch B receives. In the Isochronous Matrix, locks are abolished.

* **Phase/Tick Separation:** * All `chan_send` operations execute in Tick $N$. The data is written to a shadow buffer.
  * All `chan_recv` operations execute in Tick $N+1$. They read from the primary buffer.
* **The Result:** Massive speed. Timelines never block each other. They operate entirely in parallel CPU caches and only flush to shared memory at the exact boundary of the slice.

## 4. Syntax Proposal

```ictl
@0ms: {
  // Define an extremely fast isolate running at a strict 5ms heartbeat (200Hz)
  isolate hft_engine {
    slice 5ms 
    
    // We open a lock-free double-buffered channel
    open_chan signal_bus(100)
    
    split main into [market_reader, strategy_eval]
    
    @market_reader: {
      // This loops infinitely, but advances exactly ONE iteration per global tick.
      loop tick {
        let price = defer System.UDPStream(port=8080) deadline 1ms
        
        match entropy(price) {
          Valid(p): chan_send signal_bus(p) // Sent to shadow buffer
          Pending(_): require System.Log(message="No packet this tick")
        }
      }
    }
    
    @strategy_eval: {
      loop tick {
        // Reads data sent during the PREVIOUS tick with zero locking overhead
        let new_signal = chan_recv(signal_bus) 
        
        inspect(new_signal) {
          if (new_signal.value > 100) {
            call execute_trade(clone new_signal) taking 2ms
          }
        }
      }
    }
  }
}
```

## 5. Why this achieves "Extreme Speed"

1.  **Hardware Sympathy:** By fixing the time slice, the OS thread scheduler isn't constantly interrupting your VM. You are pinning threads to cores and running them in bulk synchronous parallel (BSP) mode.
2.  **O(1) EGC Execution:** Entropic Garbage Collection only runs at the tick boundary. Because all branches pause at the exact same nanosecond, the VM can bulk-drop dead memory without any complex concurrent memory management.
3.  **GPU / SIMD Readiness:** Fixed-slice execution with lock-free channel boundaries perfectly maps to how GPUs process data. If you compile ICTL with a rigid `slice`, you could theoretically cross-compile the timelines directly to compute shaders.

---

### The Engineering Reality
By forcing the CPU time slice, you transition ICTL from an interpreted language to a systems language capable of driving robotics, game engines, and financial exchanges. 