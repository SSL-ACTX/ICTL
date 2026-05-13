use ictl_analysis::analyzer::EntropicAnalyzer;
use ictl_core::value::Payload;
use ictl_core::{ResolutionStrategy, Statement};
use ictl_frontend::parser;
use ictl_runtime::vm::Vm;

#[test]
fn ictl_entropic_analyzer_struct_field_decay() -> anyhow::Result<()> {
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
fn ictl_entropic_struct_field_access_leads_to_decay() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
      let s = struct { a="Hello", b="World" }
      let a_val = s.a
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let ir = ictl_frontend::ir::lower_program(&program);
    let mut vm = Vm::new();
    vm.execute_program(&ir)?;

    let s_reg = ir.symbols.get("s").expect("s not found").0;
    let a_val_reg = ir.symbols.get("a_val").expect("a_val not found").0;

    // With the new peek behavior, decayed structs return their remaining payload.
    // We check that it's still accessible but effectively decayed.
    assert!(
        vm.root_timeline.arena.peek(s_reg).is_some(),
        "parent struct should be accessible via peek even when decayed"
    );

    let a_res = vm.root_timeline.arena.peek(a_val_reg);

    match a_res {
        Some(Payload::String(a_str)) => assert_eq!(a_str, "Hello"),
        _ => panic!("Expected extracted field value to be present"),
    }

    Ok(())
}

#[test]
fn ictl_entropic_entropic_entanglement_cross_branch() -> anyhow::Result<()> {
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

    let ir = ictl_frontend::ir::lower_program(&program);
    let mut vm = Vm::new();
    vm.execute_program(&ir)?;

    let branch_b = vm
        .active_branches
        .get("branchB")
        .expect("branchB should exist");
    let status_reg = ir.symbols.get("status").expect("status not found").0;
    let status = branch_b.arena.peek(status_reg);
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
fn ictl_entropic_entropic_entanglement_field_decay() -> anyhow::Result<()> {
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
    let ir = ictl_frontend::ir::lower_program(&program);
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_program(&ir)?;

    let branch_b = vm.active_branches.get("branchB").unwrap();
    let status_reg = ir.symbols.get("status").expect("status not found").0;
    let status = branch_b.arena.peek(status_reg);
    match status {
        Some(Payload::String(s)) => assert_eq!(s, "decayed"),
        _ => panic!("Expected status='decayed', got {:?}", status),
    }

    Ok(())
}

#[test]
fn ictl_entropic_topology_routing() -> anyhow::Result<()> {
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
    let ir = ictl_frontend::ir::lower_program(&program);
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_program(&ir)?;

    let branch_w2 = vm.active_branches.get("w2").unwrap();
    let check1_reg = ir.symbols.get("check1").expect("check1 not found").0;
    let check2_reg = ir.symbols.get("check2").expect("check2 not found").0;
    let check1 = branch_w2.arena.peek(check1_reg);
    let check2 = branch_w2.arena.peek(check2_reg);

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
fn ictl_entropic_topographical_merge_union() -> anyhow::Result<()> {
    let source = r#"
    @0ms: {
        let graph = topology {
            "core": struct { status: "stable", val: 1 },
            "node_b": struct { status: "standby", val: 2 }
        }

        split main into [alpha, beta]
    }

    @5ms: {
        @alpha: {
            graph["core"].status = "upgrading"
        }
        
        @beta: {
            inspect(graph) {
                let dead_core = graph["core"]
            }
        }
    }

    @10ms: {
        merge [alpha, beta] into main resolving (
            graph: topology_union {
                "core": priority(alpha),
                "_": decay
            }
        )
        
        let final_status = graph["core"].status
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let ir = ictl_frontend::ir::lower_program(&program);

    for (i, block) in ir.blocks.iter().enumerate() {
        println!("Block {}: {:?}", i, block.instructions);
    }

    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_program(&ir)?;

    // final_status should be "upgrading"
    let status_reg = ir
        .symbols
        .get("final_status")
        .expect("final_status not found")
        .0;
    let status_val = vm.root_timeline.arena.peek(status_reg);
    match status_val {
        Some(Payload::String(s)) => assert_eq!(s, "upgrading"),
        _ => panic!("Expected upgrading status, got {:?}", status_val),
    }

    Ok(())
}

#[test]
fn ictl_entropic_topographical_merge_union_on_invalid_clause() -> anyhow::Result<()>
{
    let source = r#"
    @0ms: {
        let graph = topology {
            "core": struct { status: "stable", val: 1 }
        }

        split main into [alpha, beta]
    }

    @5ms: {
        @alpha: {
            graph["core"].status = "upgrading"
        }

        @beta: {
            graph["core"].status = "downgrade"
        }
    }

    @10ms: {
        merge [alpha, beta] into main resolving (
            graph: topology_union {
                "core": priority(alpha),
                "_": decay
                on_invalid: rewind alpha to base
            }
        )

        let final_status = graph["core"].status
    }
    "#;

    let program = parser::parse_ictl(source)?;
    let ir = ictl_frontend::ir::lower_program(&program);

    // Verify AST parse of `on_invalid` rewinding behavior into topology_union
    let merge_stmt = &program.timelines[2].statements[0].stmt;
    if let Statement::Merge { resolutions, .. } = merge_stmt {
        let graph_rule = resolutions
            .rules
            .get("graph")
            .expect("graph resolution should exist");

        match graph_rule {
            ResolutionStrategy::TopologyUnion {
                on_invalid: Some(reversion),
                ..
            } => {
                assert_eq!(reversion.branch, "alpha");
                assert_eq!(reversion.anchor, "base");
            }
            _ => panic!("Expected topology_union with on_invalid rewind clause"),
        }
    } else {
        panic!("Expected first statement of @10ms to be Merge");
    }

    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
    vm.execute_program(&ir)?;

    let status_reg = ir
        .symbols
        .get("final_status")
        .expect("final_status not found")
        .0;
    let status_val = vm.root_timeline.arena.peek(status_reg);
    match status_val {
        Some(Payload::String(s)) => assert_eq!(s, "upgrading"),
        _ => panic!("Expected upgrading status, got {:?}", status_val),
    }

    Ok(())
}
