use ictl_analysis::analyzer::EntropicAnalyzer;
use ictl_core::value::Payload;
use ictl_frontend::parser;
use ictl_runtime::vm::Vm;

#[test]
fn ictl_temporal_parse_analyze_execute_timeline() -> anyhow::Result<()> {
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
fn ictl_temporal_if_equalizes_timing() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      if (1 == 0) {
        network_request "api.example.com"
      } else {
        let x = "hi"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.register_capability("System.NetworkFetch", |_| Ok(()));
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    assert_eq!(vm.root_timeline.local_clock, 8); // condition + padding -> 8 total
    Ok(())
}

#[test]
fn ictl_temporal_loop_break_pads_to_max() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      loop (max 10ms) {
        let x = "a"
        break
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

    assert_eq!(vm.root_timeline.local_clock, 11); // 1 for loop stmt + pad to 10 (inclusive)
    Ok(())
}

#[test]
fn ictl_temporal_routine_call_contract_and_entropy() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let token = "secure_abc123"
      let tx = struct { amount = "100", currency = "USD" }
      routine process_payment(consume auth_token, peek transaction_details) taking 25ms {
        let amt = transaction_details.amount
        yield amt
      }
      let result = call process_payment(token, tx)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let result_value = vm.root_timeline.arena.peek("result");
    match result_value {
        Some(Payload::String(v)) => assert_eq!(v, "100"),
        _ => panic!("Expected result=\"100\""),
    }

    assert!(vm.root_timeline.arena.peek("token").is_none());
    assert!(vm.root_timeline.arena.peek("tx").is_some());
    assert_eq!(vm.root_timeline.local_clock, 28);
    Ok(())
}

#[test]
fn ictl_temporal_routine_exceeds_taking_fails_analyzer() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      routine too_slow() taking 1ms {
        network_request "api.example.com"
        let x = "ok"
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());
    Ok(())
}

#[test]
fn ictl_temporal_routine_nested_contract_fails_analyzer() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      routine inner() taking 10ms {
        let x = "hello"
      }
      routine outer() taking 5ms {
        let y = call inner()
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());
    Ok(())
}

#[test]
fn ictl_temporal_routine_split_merge_in_body_fails_analyzer() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      routine invalid() taking 10ms {
        split main into [worker]
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());
    Ok(())
}

#[test]
fn ictl_temporal_network_request_syntax_parse_and_execute() -> anyhow::Result<()> {
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
    vm.register_capability("System.NetworkFetch", |_| Ok(()));
    vm.execute_statement("main", &program.timelines[0].statements[0])?;

    let before = vm.root_timeline.cpu_budget_ms;
    vm.execute_statement("main", &program.timelines[0].statements[1])?;

    assert_eq!(vm.root_timeline.local_clock, 7);
    assert_eq!(vm.root_timeline.cpu_budget_ms, before - 5);

    Ok(())
}

#[test]
fn ictl_temporal_defer_await_success() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let dataset = defer System.NetworkFetch(url="api.data", latency="10") deadline 50ms
      await(dataset)
      print(dataset)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.register_capability("System.NetworkFetch", |_| Ok(()));
    vm.register_capability("System.Log", |_| Ok(()));
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    assert_eq!(vm.root_timeline.local_clock, 12);
    // `print` consumes its argument, so dataset should be gone after printing
    assert!(vm.root_timeline.arena.peek("dataset").is_none());
    Ok(())
}

#[test]
fn ictl_temporal_defer_await_timeout() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let ds = defer System.NetworkFetch(url="api.slow", latency="100") deadline 20ms
      await(ds)
      match entropy(ds) {
        Pending(p): { let r = "pending" }
        Valid(v): { let r = "valid" }
        Consumed: { let r = "consumed" }
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

    let result = vm.root_timeline.arena.peek("r");
    match result {
        Some(Payload::String(s)) => assert_eq!(s, "consumed"),
        _ => panic!("Expected consumed branch"),
    }
    Ok(())
}

#[test]
fn ictl_temporal_relativistic_network_request_merge() -> anyhow::Result<()> {
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
    vm.register_capability("System.NetworkFetch", |_| Ok(()));
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
fn ictl_temporal_isolate_manifest_cpu_limit_reflects_in_vm() -> anyhow::Result<()> {
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
fn ictl_temporal_for_loop_pacing_and_bounds() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let arr = ["a","b","c"]
      for x consume arr pacing 5ms (max 20ms) {
        let y = x
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

    assert_eq!(vm.root_timeline.local_clock, 20);
    Ok(())
}
