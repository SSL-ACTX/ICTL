use ictl_analysis::analyzer::EntropicAnalyzer;
use ictl_core::value::{Arena, EntropicState, Payload};
use ictl_frontend::parser;
use ictl_runtime::vm::{TemporalError, Vm};

#[test]
fn ictl_semantic_arena_insert_overwrite_reclaims_previous_memory() {
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
fn ictl_semantic_if_statement_integer_arith() -> anyhow::Result<()> {
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
fn ictl_semantic_type_system_assignment_mismatch() -> anyhow::Result<()> {
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
fn ictl_semantic_type_system_if_condition_must_be_bool() -> anyhow::Result<()> {
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
fn ictl_semantic_type_annotation_assignment_matches() -> anyhow::Result<()> {
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
fn ictl_semantic_type_annotation_assignment_mismatch() -> anyhow::Result<()> {
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
fn ictl_semantic_type_decl_and_custom_type_assignment() -> anyhow::Result<()> {
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
fn ictl_semantic_type_decl_assignment_mismatch() -> anyhow::Result<()> {
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
fn ictl_semantic_routine_param_return_types() -> anyhow::Result<()> {
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
fn ictl_semantic_inspect_block_does_not_consume() -> anyhow::Result<()> {
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
fn ictl_semantic_if_reconcile_auto() -> anyhow::Result<()> {
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
fn ictl_semantic_routine_taking_inferred() -> anyhow::Result<()> {
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
fn ictl_semantic_match_entropy_valid_branch() -> anyhow::Result<()> {
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
fn ictl_semantic_routine_consume_non_identifier_fails_analyzer() -> anyhow::Result<()>
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
fn ictl_semantic_routine_yield_array_struct_return() -> anyhow::Result<()> {
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
fn ictl_semantic_if_requires_reconcile_for_crosspath_consume() -> anyhow::Result<()>
{
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
fn ictl_semantic_merge_resolution_first_wins() -> anyhow::Result<()> {
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
fn ictl_semantic_analyzer_missing_capability_block() -> anyhow::Result<()> {
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
fn ictl_semantic_isolate_print_requires_system_log() -> anyhow::Result<()> {
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
fn ictl_semantic_isolate_print_with_system_log() -> anyhow::Result<()> {
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
            ictl_core::TimeCoordinate::Global(_) => "main",
            ictl_core::TimeCoordinate::Relative(_) => "main",
            ictl_core::TimeCoordinate::Branch(name) => name.as_str(),
        };
        for stmt in &timeline.statements {
            vm.execute_statement(branch, stmt)?;
        }
    }

    Ok(())
}

#[test]
fn ictl_semantic_isolate_print_without_system_log_handler_fails(
) -> anyhow::Result<()> {
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
            ictl_core::TimeCoordinate::Global(_) => "main",
            ictl_core::TimeCoordinate::Relative(_) => "main",
            ictl_core::TimeCoordinate::Branch(name) => name.as_str(),
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
fn ictl_semantic_for_struct_iteration_source() -> anyhow::Result<()> {
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
fn ictl_semantic_file_input_pipeline() -> anyhow::Result<()> {
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
            ictl_core::TimeCoordinate::Global(_) => "main",
            ictl_core::TimeCoordinate::Relative(_) => "main",
            ictl_core::TimeCoordinate::Branch(name) => name.as_str(),
        };
        for stmt in &timeline.statements {
            vm.execute_statement(branch, stmt)?;
        }
    }

    assert!(vm.root_timeline.arena.peek("x").is_some());

    Ok(())
}

#[test]
fn ictl_semantic_print_statement() -> anyhow::Result<()> {
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
            ictl_core::TimeCoordinate::Global(_) => "main",
            ictl_core::TimeCoordinate::Relative(_) => "main",
            ictl_core::TimeCoordinate::Branch(name) => name.as_str(),
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
fn ictl_semantic_debug_log_non_consuming() -> anyhow::Result<()> {
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
            ictl_core::TimeCoordinate::Global(_) => "main",
            ictl_core::TimeCoordinate::Relative(_) => "main",
            ictl_core::TimeCoordinate::Branch(name) => name.as_str(),
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
fn ictl_semantic_isolate_memory_limit_out_of_memory() -> anyhow::Result<()> {
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
fn ictl_semantic_channel_send_receive() -> anyhow::Result<()> {
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
fn ictl_semantic_clone_and_reuse_variable() -> anyhow::Result<()> {
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
fn ictl_semantic_gc_terminate_branch() -> anyhow::Result<()> {
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
fn ictl_semantic_gc_merge_collects_leaf_branches() -> anyhow::Result<()> {
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
fn ictl_semantic_capability_require_outbound_and_use() -> anyhow::Result<()> {
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
fn ictl_semantic_analyzer_unresolved_merge_collision() -> anyhow::Result<()> {
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
fn ictl_semantic_analyzer_use_after_consume() -> anyhow::Result<()> {
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
fn ictl_semantic_channel_receive_from_empty_channel_fails() -> anyhow::Result<()> {
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
fn ictl_semantic_merge_priority_resolves_to_priority_branch() -> anyhow::Result<()> {
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
fn ictl_semantic_split_map_collects_yields() -> anyhow::Result<()> {
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
fn ictl_semantic_capability_budget_enforcement() -> anyhow::Result<()> {
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
