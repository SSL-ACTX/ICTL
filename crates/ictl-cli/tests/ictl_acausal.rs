use ictl_analysis::analyzer::EntropicAnalyzer;
use ictl_core::value::Payload;
use ictl_frontend::parser;
use ictl_runtime::vm::{TemporalError, Vm};

#[test]
fn ictl_acausal_speculate_commit_fallback_timing() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x = "hello"
      speculate (max 3ms) {
        let y = x
        commit {
          let out = y
        }
      } fallback {
        let out = "fallback"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let out_val = vm.root_timeline.arena.peek("out");
    match out_val {
        Some(Payload::String(s)) => assert_eq!(s, "hello"),
        _ => panic!("Expected out=hello"),
    }

    assert_eq!(vm.root_timeline.local_clock, 7);
    // 1 for initial let x, 1 for speculate statement overhead, max 3ms speculation budget, plus 1 fallback cost estimate
    Ok(())
}

#[test]
fn ictl_acausal_speculate_runs_fallback_on_collapse() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      speculate (max 3ms) {
        collapse
      } fallback {
        let out = "fallback"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let out_val = vm.root_timeline.arena.peek("out");
    match out_val {
        Some(Payload::String(s)) => assert_eq!(s, "fallback"),
        _ => panic!("Expected out=fallback"),
    }

    assert_eq!(vm.root_timeline.local_clock, 6);
    // 1 for speculate statement overhead, max 3ms speculation budget, fallback body cost 1
    Ok(())
}

#[test]
fn ictl_acausal_speculate_commit_scoped_variables() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      speculation_mode(full)
      speculate (max 3ms) {
        let secret = "hidden"
        commit {
          let out = "committed"
        }
      } fallback {
        let out = "fallback"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let out_val = vm.root_timeline.arena.peek("out");
    match out_val {
        Some(Payload::String(s)) => assert_eq!(s, "committed"),
        _ => panic!("Expected out=committed"),
    }

    let secret_val = vm.root_timeline.arena.peek("secret");
    match secret_val {
        Some(Payload::String(s)) => assert_eq!(s, "hidden"),
        _ => panic!("Expected secret=hidden in full speculative commit semantics"),
    }

    assert_eq!(vm.root_timeline.local_clock, 7);
    // cost: mode statement + speculate statement overhead + max 3ms + fallback cost 1
    Ok(())
}

#[test]
fn ictl_acausal_speculate_selective_commit_mode() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      speculation_mode(selective)
      speculate (max 3ms) {
        let secret = "hidden"
        commit {
          let out = "committed"
        }
      } fallback {
        let out = "fallback"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.set_speculative_commit_mode(ictl_core::SpeculationCommitMode::Selective);
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let out_val = vm.root_timeline.arena.peek("out");
    match out_val {
        Some(Payload::String(s)) => assert_eq!(s, "committed"),
        _ => panic!("Expected out=committed"),
    }

    assert!(vm.root_timeline.arena.peek("secret").is_none());
    assert_eq!(vm.root_timeline.local_clock, 7);
    Ok(())
}

#[test]
fn ictl_acausal_select_case_first_chan_ready() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let msg = "hello"
      open_chan c(2)
      chan_send c(msg)
      select (max 10ms) {
        case data = chan_recv(c):
          let out = data
        timeout:
          let out = "timeout"
      } reconcile (out=first_wins)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let out_val = vm.root_timeline.arena.peek("out");
    match out_val {
        Some(Payload::String(s)) => assert_eq!(s, "hello"),
        _ => panic!("Expected out=hello"),
    }

    Ok(())
}

#[test]
fn ictl_acausal_select_timeout_runs() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      select (max 5ms) {
        case data = chan_recv(c):
          let out = data
        timeout:
          let out = "timeout"
      } reconcile (out=first_wins)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let out_val = vm.root_timeline.arena.peek("out");
    match out_val {
        Some(Payload::String(s)) => assert_eq!(s, "timeout"),
        _ => panic!("Expected out=timeout"),
    }

    Ok(())
}

#[test]
fn ictl_acausal_watchdog_and_acausal_reset() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      split main into [worker]
      @worker: {
        anchor start
        let x = "work"
      }
    }
    @10ms: {
      watchdog worker timeout 1ms recovery {
        reset worker to start
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?; // split
    vm.execute_statement("worker", &program.timelines[0].statements[1])?; // anchor + x
    assert!(vm.active_branches.contains_key("worker"));

    let worker_before = vm.active_branches.get("worker").unwrap();
    assert!(worker_before.local_clock > 1);

    let _ = vm.execute_statement("main", &program.timelines[1].statements[0]);

    let worker_after = vm.active_branches.get("worker").unwrap();
    assert_eq!(worker_after.local_clock, 2);

    Ok(())
}

#[test]
fn ictl_acausal_acausal_reset_direct() -> anyhow::Result<()> {
    let source = r#"
    @0ms: { split main into [w] }
    @w: {
      anchor start
      let x = "foo"
      let x2 = x
    }
    @0ms: { reset w to start }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    vm.execute_statement("w", &program.timelines[1].statements[0])?;
    vm.execute_statement("w", &program.timelines[1].statements[1])?;
    vm.execute_statement("w", &program.timelines[1].statements[2])?;
    vm.execute_statement("main", &program.timelines[2].statements[0])?;

    let worker = vm.active_branches.get("w").unwrap();
    assert_eq!(worker.local_clock, 1);

    Ok(())
}

#[test]
fn ictl_acausal_commit_then_rewind_fails() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      split main into [w]
    }
    @w: {
      anchor start
      let x = "once"
      commit {
        let y = "two"
      }
      rewind_to(start)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?; // split
    vm.execute_statement("w", &program.timelines[1].statements[0])?; // anchor
    vm.execute_statement("w", &program.timelines[1].statements[1])?; // let x
    vm.execute_statement("w", &program.timelines[1].statements[2])?; // commit

    let res = vm.execute_statement("w", &program.timelines[1].statements[3]);
    assert!(res.is_err(), "rewind should fail after commit horizon");

    let w_after_commit = vm.active_branches.get("w").unwrap();
    assert!(
        w_after_commit.anchors.is_empty(),
        "anchors must be cleared after commit"
    );

    Ok(())
}

#[test]
fn ictl_acausal_commit_clears_anchor_snapshots() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      split main into [w]
    }
    @w: {
      anchor start
      let x = "once"
      commit {
        let y = "two"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    vm.execute_statement("w", &program.timelines[1].statements[0])?; // anchor
    vm.execute_statement("w", &program.timelines[1].statements[1])?; // let x
    vm.execute_statement("w", &program.timelines[1].statements[2])?; // commit

    let w = vm.active_branches.get("w").unwrap();
    assert!(w.anchors.is_empty());
    assert!(w.commit_horizon_passed);

    Ok(())
}

#[test]
fn ictl_acausal_rewind_in_chaos_mode_fails_analyzer() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate net {
        require System.Entropy(mode="chaos")
        anchor start
        rewind_to(start)
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    let res = analyzer.analyze_program(&program);
    assert!(res.is_err());

    Ok(())
}

#[test]
fn ictl_acausal_rewind_restores_clock() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x = 1
      anchor start
      let y = 2
      let z = 3
      rewind_to(start)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for timeline in &program.timelines {
        for stmt in &timeline.statements {
            vm.execute_statement("main", stmt)?;
        }
    }

    // After rewind, local_clock should be back at 'start' anchor
    // initial 'let x=1' is 1ms. 'anchor' is 1ms.
    // So 'start' anchor clock should be 2.
    assert_eq!(vm.root_timeline.local_clock, 2);
    // y and z should be gone from arena
    assert!(vm.root_timeline.arena.peek("y").is_none());
    assert!(vm.root_timeline.arena.peek("z").is_none());
    assert!(vm.root_timeline.arena.peek("x").is_some());

    Ok(())
}

#[test]
fn ictl_acausal_causal_paradox_on_consumed_send() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      open_chan c(1)
      split main into [w1, w2]
    }
      
    @w1: {
      anchor a1
      let msg = "hello"
      chan_send c(msg)
      // w1 will try to rewind after w2 consumes
    }
      
    @w2: {
      let got = chan_recv(c)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    // 1. open_chan
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    // 2. split
    vm.execute_statement("main", &program.timelines[0].statements[1])?;

    // 3. w1: anchor a1, let msg, chan_send
    vm.execute_statement("w1", &program.timelines[1].statements[0])?;
    vm.execute_statement("w1", &program.timelines[1].statements[1])?;
    vm.execute_statement("w1", &program.timelines[1].statements[2])?;

    // 4. w2: chan_recv (consumes)
    vm.execute_statement("w2", &program.timelines[2].statements[0])?;

    // 5. w1: rewind_to(a1) -> Should FAIL with Paradox
    let rewind_stmt = ictl_core::SpannedStatement {
        stmt: ictl_core::Statement::Rewind("a1".to_string()),
        span: ictl_core::Span { start: 0, end: 0 },
    };

    let result = vm.execute_statement("w1", &rewind_stmt);
    match result {
        Err(TemporalError::Paradox) => {}
        other => panic!("Expected Paradox error, got {:?}", other),
    }

    Ok(())
}

#[test]
fn ictl_acausal_safe_unsend_on_rewind() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      open_chan c(1)
      anchor a1
      let msg = "hello"
      chan_send c(msg)
      rewind_to(a1)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for timeline in &program.timelines {
        for stmt in &timeline.statements {
            vm.execute_statement("main", stmt)?;
        }
    }

    // After rewind, channel 'c' should be EMPTY because we un-sent the message
    let chan = vm.channels.get("c").unwrap();
    assert!(chan.is_empty());

    Ok(())
}

#[test]
fn ictl_acausal_safe_unrecv_on_rewind() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      open_chan c(1)
      let msg = "hello"
      chan_send c(msg)
      
      anchor a1
      let got = chan_recv(c)
      rewind_to(a1)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for timeline in &program.timelines {
        for stmt in &timeline.statements {
            vm.execute_statement("main", stmt)?;
        }
    }

    // After rewind, channel 'c' should have "hello" back
    let chan = vm.channels.get("c").unwrap();
    assert_eq!(chan.len(), 1);
    match &chan[0].payload {
        Payload::String(s) => assert_eq!(s, "hello"),
        _ => panic!("Expected string payload"),
    }

    Ok(())
}
