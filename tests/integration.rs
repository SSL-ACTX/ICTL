use ictl::analysis::analyzer::EntropicAnalyzer;
use ictl::frontend::parser;
use ictl::runtime::memory::{Arena, EntropicState, Payload};
use ictl::runtime::vm::{TemporalError, Vm};

#[test]
fn integration_arena_insert_overwrite_reclaims_previous_memory() {
    let mut arena = Arena::new(200);
    // Key "x" weight: 1 + 32 = 33
    // Payload "abc" weight: 3 + 24 = 27
    // EntropicState::Valid overhead: 16
    // Total: 33 + 27 + 16 = 76
    assert!(arena
        .insert(
            "x".to_string(),
            EntropicState::Valid(Payload::String("abc".into()))
        )
        .is_ok());
    assert_eq!(arena.used, 76);

    // Payload "abcdefgh" weight: 8 + 24 = 32
    // EntropicState::Valid overhead: 16
    // Total: 33 + 32 + 16 = 81
    assert!(arena
        .insert(
            "x".to_string(),
            EntropicState::Valid(Payload::String("abcdefgh".into()))
        )
        .is_ok());
    assert_eq!(arena.used, 81);
}

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
fn integration_if_statement_integer_arith() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let a = 10
      let b = 20
      if (a < b) {
        let c = 1
      } else {
        let c = 0
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

    let c_val = vm.root_timeline.arena.peek("c");
    match c_val {
        Some(Payload::Integer(v)) => assert_eq!(v, 1),
        _ => panic!("Expected c=1 in branch"),
    }

    Ok(())
}

#[test]
fn integration_type_system_assignment_mismatch() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x = 1
      let x = "oops"
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());

    Ok(())
}

#[test]
fn integration_type_system_if_condition_must_be_bool() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x = 1
      if (x) {
        let y = 2
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());

    Ok(())
}

#[test]
fn integration_type_annotation_assignment_matches() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x: int = 1
      let y: bool = false
      let z = x + 2
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;
    Ok(())
}

#[test]
fn integration_type_annotation_assignment_mismatch() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x: bool = 1
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());
    Ok(())
}

#[test]
fn integration_type_decl_and_custom_type_assignment() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      type Point = struct { x:int, y:int }
      let p: Point = struct { x = 3, y = 4 }
      let s = p.x + p.y
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;
    Ok(())
}

#[test]
fn integration_type_decl_assignment_mismatch() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      type Point = struct { x:int, y:int }
      let p: Point = struct { x = 3, z = 4 }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());
    Ok(())
}

#[test]
fn integration_routine_param_return_types() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      routine add(consume a:int, consume b:int) -> int taking _ {
        let sum = a + b
        yield sum
      }
      let result:int = call add(10, 20)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let result_val = vm.root_timeline.arena.peek("result");
    match result_val {
        Some(Payload::Integer(v)) => assert_eq!(v, 30),
        _ => panic!("Expected result=30"),
    }
    Ok(())
}

#[test]
fn integration_inspect_block_does_not_consume() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let p = struct { a = "x", b = "y" }
      inspect(p) {
        let x = p.a
        let y = p.b
      }
      let z = p
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let z = vm.root_timeline.arena.peek("z");
    assert!(z.is_some());
    Ok(())
}

#[test]
fn integration_if_reconcile_auto() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let v = 1
      if (v > 0) {
        let x = 5
      } else {
        let x = 10
      } reconcile auto
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let x_val = vm.root_timeline.arena.peek("x");
    assert!(x_val.is_some());
    Ok(())
}

#[test]
fn integration_routine_taking_inferred() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      routine f(peek p) taking _ {
        let q = p
      }
      let s = "ok"
      let r = call f(s)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let r_val = vm.root_timeline.arena.peek("r");
    assert!(matches!(r_val, Some(Payload::String(_))));
    Ok(())
}

#[test]
fn integration_if_equalizes_timing() -> anyhow::Result<()> {
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
fn integration_speculate_commit_fallback_timing() -> anyhow::Result<()> {
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
fn integration_speculate_runs_fallback_on_collapse() -> anyhow::Result<()> {
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
fn integration_speculate_commit_scoped_variables() -> anyhow::Result<()> {
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
fn integration_speculate_selective_commit_mode() -> anyhow::Result<()> {
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
    vm.set_speculative_commit_mode(
        ictl::frontend::ast::SpeculationCommitMode::Selective,
    );
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
fn integration_select_case_first_chan_ready() -> anyhow::Result<()> {
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
fn integration_select_timeout_runs() -> anyhow::Result<()> {
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
fn integration_match_entropy_valid_branch() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let user = struct { id = "1", name = "Alice" }
      match entropy(user) {
        Valid(u):
          let out = u.id
        Consumed:
          let out = "consumed"
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
        Some(Payload::String(s)) => assert_eq!(s, "1"),
        _ => panic!("Expected out=1"),
    }

    Ok(())
}

#[test]
fn integration_loop_break_pads_to_max() -> anyhow::Result<()> {
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
fn integration_loop_tick_requires_slice() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      loop tick {
        let x = 1
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    let res = analyzer.analyze_program(&program);
    assert!(res.is_err(), "loop tick without slice should fail analyzer");

    Ok(())
}

#[test]
fn integration_loop_tick_slice_budget_enforced() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate demo {
        slice 2ms
        loop tick {
          let x = 1
          let y = 2
          let z = 3
          break
        }
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    let res = analyzer.analyze_program(&program);
    assert!(
        res.is_err(),
        "loop tick body exceeds slice should fail analyzer"
    );

    Ok(())
}

#[test]
fn integration_tick_loop_double_buffered_channels() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate demo {
        slice 5ms
        open_chan c(10)

        loop tick {
          let v = 42
          chan_send c(v)
          break
        }

        loop tick {
          let out = chan_recv(c)
          break
        }
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

    match vm.root_timeline.arena.peek("out") {
        Some(Payload::Integer(v)) => assert_eq!(v, 42),
        _ => panic!("Expected out=42"),
    }

    Ok(())
}

#[test]
fn integration_routine_call_contract_and_entropy() -> anyhow::Result<()> {
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
fn integration_routine_exceeds_taking_fails_analyzer() -> anyhow::Result<()> {
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
fn integration_routine_nested_contract_fails_analyzer() -> anyhow::Result<()> {
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
fn integration_routine_split_merge_in_body_fails_analyzer() -> anyhow::Result<()> {
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
fn integration_routine_consume_non_identifier_fails_analyzer() -> anyhow::Result<()>
{
    let source = r#"
    @0ms: {
      routine fn(consume token) taking 5ms {
        yield token
      }
      let result = call fn("not_var", "x")
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());
    Ok(())
}

#[test]
fn integration_routine_yield_array_struct_return() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      routine make_res() taking 20ms {
        let a = [1,2,3]
        let s = struct { x = "hello", y = "world" }
        yield a
        yield s
      }
      let result1 = call make_res()
      let result2 = call make_res()
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    assert!(vm.root_timeline.arena.peek("result1").is_some());
    assert!(vm.root_timeline.arena.peek("result2").is_some());
    Ok(())
}

#[test]
fn integration_if_requires_reconcile_for_crosspath_consume() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x = "foo"
      if (1 == 1) {
        let y = x
      } else {
        let z = "bar"
      }
    }
    "#; // no reconcile

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(analyzer.analyze_program(&program).is_err());

    let source_with_reconcile = r#"
    @0ms: {
      let x = "foo"
      if (1 == 1) {
        let y = x
      } else {
        let z = "bar"
      } reconcile (x=first_wins)
    }
    "#;

    let program = parser::parse_ictl(source_with_reconcile)?;
    analyzer.analyze_program(&program)?;
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
fn integration_isolate_print_requires_system_log() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate demo {
        enable cpu(10)
        let msg = "hello"
        print(msg)
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    assert!(
        analyzer.analyze_program(&program).is_err(),
        "Print in isolate requires System.Log"
    );

    Ok(())
}

#[test]
fn integration_isolate_print_with_system_log() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate demo {
        enable cpu(10)
        require System.Log
        let msg = "hello"
        print(msg)
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

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

    Ok(())
}

#[test]
fn integration_isolate_print_without_system_log_handler_fails() -> anyhow::Result<()>
{
    let source = r#"
    @0ms: {
      isolate demo {
        enable cpu(10)
        require System.Log
        let msg = "hello"
        print(msg)
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();

    let mut actual_err = None;
    for timeline in &program.timelines {
        let branch = match &timeline.time {
            ictl::frontend::ast::TimeCoordinate::Global(_) => "main",
            ictl::frontend::ast::TimeCoordinate::Relative(_) => "main",
            ictl::frontend::ast::TimeCoordinate::Branch(name) => name.as_str(),
        };
        for stmt in &timeline.statements {
            if let Err(e) = vm.execute_statement(branch, stmt) {
                actual_err = Some(e);
                break;
            }
        }
        if actual_err.is_some() {
            break;
        }
    }

    match actual_err {
        Some(TemporalError::MissingCapability(path)) => {
            assert_eq!(path, "System.Log");
        }
        Some(e) => panic!("Unexpected runtime error: {e:?}"),
        None => panic!("Expected missing capability runtime error"),
    }

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
    vm.register_capability("System.NetworkFetch", |_| Ok(()));
    vm.execute_statement("main", &program.timelines[0].statements[0])?;

    let before = vm.root_timeline.cpu_budget_ms;
    vm.execute_statement("main", &program.timelines[0].statements[1])?;

    assert_eq!(vm.root_timeline.local_clock, 7);
    assert_eq!(vm.root_timeline.cpu_budget_ms, before - 5);

    Ok(())
}

#[test]
fn integration_defer_await_success() -> anyhow::Result<()> {
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
fn integration_defer_await_timeout() -> anyhow::Result<()> {
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
fn integration_for_struct_iteration_source() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let raw = struct { a = "1", b = "2" }
      for item consume raw {
        let item_copy = clone(item)
        let key = item.key
        let value = item_copy.value
        let produced = struct { key = key, value = value }
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
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
fn integration_print_statement() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let msg = "hello"
      print(msg)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.register_capability("System.Log", |_| Ok(()));
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

    // After print, msg has been consumed by expression semantics
    assert!(vm.root_timeline.arena.peek("msg").is_none());

    Ok(())
}

#[test]
fn integration_debug_log_non_consuming() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let v = "hello"
      debug(v)
      log(v)
      let x = clone(v)
      let y = clone(x)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.register_capability("System.Log", |_| Ok(()));
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

    // v must survive debug/log and be cloneable
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
fn integration_commit_clears_anchor_snapshots() -> anyhow::Result<()> {
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
fn integration_rewind_in_chaos_mode_fails_analyzer() -> anyhow::Result<()> {
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

    // With the new peek behavior, decayed structs return their remaining payload.
    // We check that it's still accessible but effectively decayed.
    assert!(
        vm.root_timeline.arena.peek("s").is_some(),
        "parent struct should be accessible via peek even when decayed"
    );

    let a_res = vm.root_timeline.arena.peek("a_val");

    match a_res {
        Some(Payload::String(a_str)) => assert_eq!(a_str, "Hello"),
        _ => panic!("Expected extracted field value to be present"),
    }

    Ok(())
}

#[test]
fn integration_for_loop_pacing_and_bounds() -> anyhow::Result<()> {
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

#[test]
fn integration_split_map_collects_yields() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let data = [1,2,3]
      split_map item consume data {
        yield item
      } reconcile (result=first_wins)
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    for stmt in &program.timelines[0].statements {
        vm.execute_statement("main", stmt)?;
    }

    let out = vm.root_timeline.arena.peek("splitmap_results");
    assert!(out.is_some());
    Ok(())
}

#[test]
fn integration_entropic_entanglement_cross_branch() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let x = "shared"
      entangle(x)
      split main into [branchA, branchB]
      @branchA: {
        let use_x = x
      }
      @branchB: {
        match entropy(x) {
          Valid(v):
            let status = "alive"
          Consumed:
            let status = "dead"
        }
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    // 1. let x = "shared"
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    // 2. entangle x
    vm.execute_statement("main", &program.timelines[0].statements[1])?;
    // 3. split main into [branchA, branchB]
    vm.execute_statement("main", &program.timelines[0].statements[2])?;

    // 4. In branchA, consume x
    vm.execute_statement("branchA", &program.timelines[0].statements[3])?;

    // 5. In branchB, check if x is dead
    vm.execute_statement("branchB", &program.timelines[0].statements[4])?;

    let branch_b = vm.active_branches.get("branchB").unwrap();
    let status = branch_b.arena.peek("status");
    match status {
        Some(Payload::String(s)) => assert_eq!(s, "dead"),
        _ => panic!(
            "Expected status='dead' due to entanglement propagation, got {:?}",
            status
        ),
    }

    Ok(())
}

#[test]
fn integration_entropic_entanglement_field_decay() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let p = struct { a = "1", b = "2" }
      entangle(p)
      split main into [branchA, branchB]
      @branchA: {
        let val = p.a
      }
      @branchB: {
        match entropy(p) {
          Decayed(d):
            let status = "decayed"
          Valid(v):
            let status = "still_valid"
          Consumed:
            let status = "consumed"
        }
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    // 1. let p = ...
    vm.execute_statement("main", &program.timelines[0].statements[0])?;
    // 2. entangle(p)
    vm.execute_statement("main", &program.timelines[0].statements[1])?;
    // 3. split
    vm.execute_statement("main", &program.timelines[0].statements[2])?;

    // 4. In branchA, let val = p.a
    vm.execute_statement("branchA", &program.timelines[0].statements[3])?;

    // 5. In branchB, check status
    vm.execute_statement("branchB", &program.timelines[0].statements[4])?;

    let branch_b = vm.active_branches.get("branchB").unwrap();
    let status = branch_b.arena.peek("status");
    match status {
        Some(Payload::String(s)) => assert_eq!(s, "decayed"),
        _ => panic!("Expected status='decayed', got {:?}", status),
    }

    Ok(())
}

#[test]
fn integration_topology_routing() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let network = topology {
        node1 = "up",
        node2 = "up"
      }
      entangle(network)
      split main into [w1, w2]
      @w1: {
        let n1_status = network["node1"]
      }
      @w2: {
        match entropy(network["node1"]) {
           Consumed: let check1 = "offline"
           Valid(v): let check1 = "online"
        }
        match entropy(network["node2"]) {
           Valid(v): let check2 = "online"
           Consumed: let check2 = "offline"
        }
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    // execute everything
    for tl in &program.timelines {
        for stmt in &tl.statements {
            vm.execute_statement("main", stmt)?;
        }
    }

    let branch_w2 = vm.active_branches.get("w2").unwrap();
    let check1 = branch_w2.arena.peek("check1");
    let check2 = branch_w2.arena.peek("check2");

    match check1 {
        Some(Payload::String(s)) => assert_eq!(s, "offline"),
        _ => panic!("Expected check1='offline', got {:?}", check1),
    }
    match check2 {
        Some(Payload::String(s)) => assert_eq!(s, "online"),
        _ => panic!("Expected check2='online', got {:?}", check2),
    }

    Ok(())
}

#[test]
fn integration_capability_budget_enforcement() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      isolate limited {
        require System.Log
        enable system_log(1)
        print("First")
        print("Second")
      }
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.register_capability("System.Log", |_params| Ok(()));

    let result = vm.execute_statement("main", &program.timelines[0].statements[0]);
    match result {
        Err(TemporalError::CapabilityViolation(msg)) => {
            assert!(msg.contains("Capability budget exhausted"));
        }
        other => panic!("Expected CapabilityViolation, got {:?}", other),
    }

    Ok(())
}
