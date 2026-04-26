use ictl::analysis::analyzer::EntropicAnalyzer;
use ictl::frontend::parser;
use ictl::runtime::memory::Payload;
use ictl::runtime::vm::{TemporalError, Vm};

#[test]
fn test_rewind_restores_clock() -> anyhow::Result<()> {
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
fn test_causal_paradox_on_consumed_send() -> anyhow::Result<()> {
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
    let rewind_stmt = ictl::frontend::ast::SpannedStatement {
        stmt: ictl::frontend::ast::Statement::Rewind("a1".to_string()),
        span: ictl::frontend::ast::Span { start: 0, end: 0 },
    };

    let result = vm.execute_statement("w1", &rewind_stmt);
    match result {
        Err(TemporalError::Paradox) => {}
        other => panic!("Expected Paradox error, got {:?}", other),
    }

    Ok(())
}

#[test]
fn test_safe_unsend_on_rewind() -> anyhow::Result<()> {
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
fn test_safe_unrecv_on_rewind() -> anyhow::Result<()> {
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
