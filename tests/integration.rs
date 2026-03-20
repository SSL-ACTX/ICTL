use ictl::analysis::analyzer::EntropicAnalyzer;
use ictl::frontend::parser;
use ictl::runtime::memory::Payload;
use ictl::runtime::vm::{TemporalError, Vm};

#[test]
fn integration_parse_analyze_execute_timeline() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      split main into [worker]
      @worker: {
        anchor start
        let x = "hello"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;

    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;

    assert!(vm.active_branches.contains_key("worker"));
    vm.execute_statement("worker", &program.timelines[0].statements[1])?;

    let worker = vm.active_branches.get("worker").unwrap();
    assert_eq!(worker.local_clock, 3); // split+relative block+anchor+assignment each cost 1ms

    Ok(())
}

#[test]
fn integration_merge_resolution_first_wins() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      split main into [w1,w2]
      @w1: {
        let v = "v1"
      }
      @w2: {
        let v = "v2"
      }
      merge [w1,w2] into main resolving(v=w1)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?; // split
    vm.execute_statement("w1", &program.timelines[0].statements[1])?;
    vm.execute_statement("w2", &program.timelines[0].statements[2])?;
    vm.execute_statement("main", &program.timelines[0].statements[3])?; // merge

    let root_value = vm.root_timeline.arena.peek("v");
    match root_value {
        Some(Payload::String(inner)) => assert_eq!(inner, "v1"),
        _ => panic!("Expected merged v in root timeline"),
    }

    Ok(())
}

#[test]
fn integration_cli_runs_sample_file() -> anyhow::Result<()> {
    use std::process::Command;

    let out = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("--run")
        .arg("examples/sample.ictl")
        .output()?;

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("=== Compiling examples/sample.ictl ==="));
    assert!(stdout.contains("Analysis: ok"));
    assert!(stdout.contains("Run: ok"));

    Ok(())
}

#[test]
fn integration_analyzer_missing_capability_block() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate demo {
        enable cpu(10)
        let x = "hello"
        require System.IO(path="/tmp")
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    let res = analyzer.analyze_program(&program);
    assert!(res.is_err(), "Missing capability should fail analysis");

    Ok(())
}

#[test]
fn integration_watchdog_and_acausal_reset() -> anyhow::Result<()> {
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
fn integration_acausal_reset_direct() -> anyhow::Result<()> {
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
fn integration_network_request_syntax_parse_and_execute() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let a = "x"
      network_request "api.example.com"
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;

    let before = vm.root_timeline.cpu_budget_ms;
    vm.execute_statement("main", &program.timelines[0].statements[1])?;

    assert_eq!(vm.root_timeline.local_clock, 7);
    assert_eq!(vm.root_timeline.cpu_budget_ms, before - 5);

    Ok(())
}

#[test]
fn integration_relativistic_network_request_merge() -> anyhow::Result<()> {
    let source = r#"
    @0ms: { split main into [a,b] }
    @a: { network_request "api.example.com" }
    @b: { let v = "fallback" }
    @0ms: { merge [a,b] into main resolving(v=b) }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    vm.execute_statement("a", &program.timelines[1].statements[0])?;
    vm.execute_statement("b", &program.timelines[2].statements[0])?;

    let a_branch = vm.active_branches.get("a").unwrap();
    assert_eq!(a_branch.local_clock, 6);

    let b_branch = vm.active_branches.get("b").unwrap();
    assert_eq!(b_branch.local_clock, 1);

    vm.execute_statement("main", &program.timelines[3].statements[0])?;

    let root_v = vm.root_timeline.arena.peek("v");
    match root_v {
        Some(Payload::String(s)) => assert_eq!(s, "fallback"),
        _ => panic!("Expected root v to be fallback"),
    }

    Ok(())
}

#[test]
fn integration_file_input_pipeline() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate demo {
        enable cpu(10)
        require System.Log(message="hello")
        let x = "hello"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program_with_source(&program, source, "example.ictl")?;

    let mut vm = Vm::new();
    vm.register_capability("System.Log", |_params| Ok(()));

    for timeline in &program.timelines {
        let branch = match &timeline.time {
            ictl::frontend::ast::TimeCoordinate::Global(_) => "main",
            ictl::frontend::ast::TimeCoordinate::Relative(_) => "main",
            ictl::frontend::ast::TimeCoordinate::Branch(name) => name.as_str(),
        };
        for stmt in &timeline.statements {
            vm.execute_statement(branch, stmt)?;
        }
    }

    assert!(vm.root_timeline.arena.peek("x").is_some());

    Ok(())
}

#[test]
fn integration_isolate_memory_limit_out_of_memory() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate lowmem {
        enable memory(1)
        let s = "too-large"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    let exec = vm.execute_statement("main", &program.timelines[0].statements[0]);
    assert!(exec.is_err());

    Ok(())
}

#[test]
fn integration_channel_send_receive() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      open_chan c(2)
      split main into [w1,w2]
      @w1: {
        let msg = "hello"
        chan_send c(msg)
      }
      @w2: {
        let got = chan_recv(c)
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?; // open_chan
    vm.execute_statement("main", &program.timelines[0].statements[1])?; // split
    vm.execute_statement("w1", &program.timelines[0].statements[2])?;
    vm.execute_statement("w2", &program.timelines[0].statements[3])?;

    let w2 = vm.active_branches.get("w2").unwrap();
    match w2.arena.peek("got") {
        Some(Payload::String(s)) => assert_eq!(s, "hello"),
        _ => panic!("expected received string"),
    }

    Ok(())
}
#[test]
fn integration_analyzer_struct_field_decay() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let s = struct { a = "A", b = "B" }
      let x = s.a
      let y = s
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    let result = analyzer.analyze_program(&program);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn integration_commit_then_rewind_fails() -> anyhow::Result<()> {
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
    dbg!(&program.timelines);
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?; // split
    vm.execute_statement("w", &program.timelines[1].statements[0])?; // anchor
    vm.execute_statement("w", &program.timelines[1].statements[1])?; // let x
    vm.execute_statement("w", &program.timelines[1].statements[2])?; // commit

    let res = vm.execute_statement("w", &program.timelines[1].statements[3]);
    assert!(res.is_err(), "rewind should fail after commit horizon");

    Ok(())
}

#[test]
fn integration_clone_and_reuse_variable() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let a = "foo"
      let b = clone(a)
      let c = a
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let main = &vm.root_timeline;
    // `a` is consumed by c = a; b is cloned and remains available
    assert!(
        main.arena.peek("a").is_none(),
        "`a` should have been consumed by c = a"
    );
    assert!(
        main.arena.peek("b").is_some(),
        "clone result should remain available"
    );
    assert!(main.arena.peek("c").is_some(), "c should exist");
    Ok(())
}

#[test]
fn integration_gc_terminate_branch() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      split main into [w1]
    }
    @w1: {
      let v = "data"
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    vm.execute_statement("w1", &program.timelines[1].statements[0])?;

    assert!(vm.active_branches.contains_key("w1"));
    vm.terminate_branch("w1")?;
    assert!(!vm.active_branches.contains_key("w1"));
    Ok(())
}

#[test]
fn integration_gc_merge_collects_leaf_branches() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      split main into [w1,w2]
    }
    @w1: { let v1 = "x" }
    @w2: { let v2 = "y" }
    @0ms: { merge [w1,w2] into main resolving(v1=w1,v2=w2) }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    vm.execute_statement("w1", &program.timelines[1].statements[0])?;
    vm.execute_statement("w2", &program.timelines[2].statements[0])?;
    vm.execute_statement("main", &program.timelines[3].statements[0])?;

    assert!(!vm.active_branches.contains_key("w1"));
    assert!(!vm.active_branches.contains_key("w2"));

    Ok(())
}

#[test]
fn integration_capability_require_outbound_and_use() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate net {
        enable cpu(10)
        require Net.Outbound(rate="5/s", domain="api.example.com")
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.register_capability("Net.Outbound", |_params| Ok(()));
    vm.register_capability("System.Log", |_params| Ok(()));

    let res = vm.execute_statement("main", &program.timelines[0].statements[0]);
    assert!(res.is_ok());

    Ok(())
}

#[test]
fn integration_analyzer_unresolved_merge_collision() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      split main into [w1,w2]
      @w1: { let v = "v1" }
      @w2: { let v = "v2" }
      merge [w1,w2] into main
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    let result = analyzer.analyze_program(&program);
    assert!(
        result.is_err(),
        "unresolved merge collisions should trigger analyzer error"
    );
    Ok(())
}

#[test]
fn integration_analyzer_use_after_consume() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x = "a"
      let y = x
      let z = x
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    let result = analyzer.analyze_program(&program);
    assert!(
        result.is_err(),
        "use-after-consume should be rejected by analyzer"
    );
    Ok(())
}

#[test]
fn integration_isolate_manifest_cpu_limit_reflects_in_vm() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate demo {
        enable cpu(1)
        let x = "bound"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;

    assert_eq!(vm.root_timeline.cpu_budget_ms, 1);
    Ok(())
}

#[test]
fn integration_channel_receive_from_empty_channel_fails() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      open_chan c(1)
      let _ = "x"
      let recv = chan_recv(c)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    let res = vm.execute_statement("main", &program.timelines[0].statements[2]);
    assert!(matches!(res, Err(TemporalError::ChannelFault(_))));

    Ok(())
}

#[test]
fn integration_rewind_in_chaos_mode_fails_runtime() -> anyhow::Result<()> {
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
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    let res = vm.execute_statement("main", &program.timelines[0].statements[0]);
    assert!(matches!(res, Err(TemporalError::RewindDisabledInChaos)));

    Ok(())
}

#[test]
fn integration_merge_priority_resolves_to_priority_branch() -> anyhow::Result<()> {
    let source = r#"
    @0ms: { split main into [w1,w2] }
    @w1: { let v = "v1" }
    @w2: { let v = "v2" }
    @0ms: { merge [w1,w2] into main resolving(v=w2) }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    vm.execute_statement("w1", &program.timelines[1].statements[0])?;
    vm.execute_statement("w2", &program.timelines[2].statements[0])?;
    vm.execute_statement("main", &program.timelines[3].statements[0])?;

    let root_value = vm.root_timeline.arena.peek("v");
    match root_value {
        Some(Payload::String(inner)) => assert_eq!(inner, "v2"),
        _ => panic!("Expected merged v from w2"),
    }

    Ok(())
}

#[test]
fn integration_struct_field_access_leads_to_decay() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let s = struct { a="Hello", b="World" }
      let a_val = s.a
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    assert!(
        vm.root_timeline.arena.peek("s").is_none(),
        "parent struct should be decayed after field extract"
    );

    let a_res = vm.root_timeline.arena.peek("a_val");

    match a_res {
        Some(Payload::String(a_str)) => assert_eq!(a_str, "Hello"),
        _ => panic!("Expected extracted field value to be present"),
    }

    Ok(())
}
