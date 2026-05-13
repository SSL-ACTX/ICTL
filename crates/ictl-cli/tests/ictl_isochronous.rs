use ictl_analysis::analyzer::EntropicAnalyzer;
use ictl_core::value::Payload;
use ictl_frontend::parser;
use ictl_runtime::vm::Vm;

#[test]
fn ictl_isochronous_loop_tick_requires_slice() -> anyhow::Result<()> {
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
fn ictl_isochronous_loop_tick_slice_budget_enforced() -> anyhow::Result<()> {
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
fn ictl_isochronous_tick_loop_double_buffered_channels() -> anyhow::Result<()> {
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
