// tests/topographical_merge.rs
use ictl::analysis::analyzer::EntropicAnalyzer;
use ictl::frontend::ast::{ResolutionStrategy, Statement};
use ictl::frontend::parser::parse_ictl;
use ictl::runtime::memory::Payload;
use ictl::runtime::vm::Vm;

#[test]
fn integration_topographical_merge_union() -> Result<(), Box<dyn std::error::Error>>
{
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
            let dead_core = graph["core"]
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

    let program = parse_ictl(source)?;
    let mut analyzer = EntropicAnalyzer::new();
    analyzer.analyze_program(&program)?;

    let mut vm = Vm::new();
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

    // final_status should be "upgrading"
    let status_val = vm.root_timeline.arena.peek("final_status");
    match status_val {
        Some(Payload::String(s)) => assert_eq!(s, "upgrading"),
        _ => panic!("Expected upgrading status, got {:?}", status_val),
    }

    Ok(())
}

#[test]
fn integration_topographical_merge_union_on_invalid_clause() -> Result<(), Box<dyn std::error::Error>>
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

    let program = parse_ictl(source)?;

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

    let status_val = vm.root_timeline.arena.peek("final_status");
    match status_val {
        Some(Payload::String(s)) => assert_eq!(s, "upgrading"),
        _ => panic!("Expected upgrading status, got {:?}", status_val),
    }

    Ok(())
}
