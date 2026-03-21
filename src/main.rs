// src/main.rs
mod analysis;
mod frontend;
mod runtime;

use analysis::analyzer::EntropicAnalyzer;
use std::env;
use std::fs;
use std::path::PathBuf;

fn usage(program: &str) {
    eprintln!(
        "Usage: {} [--check] [--run] [--dump-ast] [--dump-ir] <file1.ictl> [file2.ictl ...]",
        program
    );
    eprintln!("  --check     Perform semantic analysis only");
    eprintln!("  --run       Execute program after analysis (default)");
    eprintln!("  --dump-ast  Print the parsed AST and continue");
    eprintln!("  --dump-ir   Print the lowered IR and continue");
}

fn format_entropic_state(state: &runtime::memory::EntropicState) -> String {
    format!("{:?}", state)
}

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        usage(&env::args().next().unwrap_or_else(|| "ictl".to_string()));
        std::process::exit(1);
    }

    let mut check_only = false;
    let mut run_program = false;
    let mut dump_ast = false;
    let mut dump_ir = false;

    while let Some(arg) = args.first() {
        if arg == "--check" {
            check_only = true;
            args.remove(0);
            continue;
        }
        if arg == "--run" {
            run_program = true;
            args.remove(0);
            continue;
        }
        if arg == "--dump-ast" {
            dump_ast = true;
            args.remove(0);
            continue;
        }
        if arg == "--dump-ir" {
            dump_ir = true;
            args.remove(0);
            continue;
        }
        break;
    }

    if !check_only && !run_program {
        run_program = true;
    }

    if args.is_empty() {
        usage(&env::args().next().unwrap_or_else(|| "ictl".to_string()));
        std::process::exit(1);
    }

    for file in args {
        let path = PathBuf::from(&file);
        let source = fs::read_to_string(&path).map_err(|e| {
            anyhow::anyhow!("Failed reading {}: {}", path.display(), e)
        })?;

        // println!("=== Compiling {} ===", path.display());

        let program = match frontend::parser::parse_ictl(&source) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Parser error for {}: {}", path.display(), e);
                continue;
            }
        };

        if dump_ast {
            println!("AST for {}:\n{:#?}", path.display(), program);
        }

        if dump_ir {
            let ir_program = frontend::ir::lower_program(&program);
            println!("IR for {}:\n{}", path.display(), ir_program);
        }

        let mut analyzer = EntropicAnalyzer::new();
        if let Err(err) = analyzer.analyze_program_with_source(
            &program,
            &source,
            &path.display().to_string(),
        ) {
            eprintln!("{}", err);
            continue;
        }

        println!("  Analysis: ok");

        if run_program {
            let mut vm = runtime::vm::Vm::new();
            vm.register_capability("System.Log", |_params| Ok(()));

            for timeline in &program.timelines {
                let branch = match &timeline.time {
                    crate::frontend::ast::TimeCoordinate::Global(_) => "main",
                    crate::frontend::ast::TimeCoordinate::Relative(_) => "main",
                    crate::frontend::ast::TimeCoordinate::Branch(name) => {
                        name.as_str()
                    }
                };

                for stmt in &timeline.statements {
                    if let Err(e) = vm.execute_statement(branch, stmt) {
                        eprintln!("Runtime error in {}: {}", path.display(), e);
                        break;
                    }
                }
            }

            println!("  Run: ok");
            println!("  Global clock: {}", vm.global_clock);
            println!("  Main local clock: {}", vm.root_timeline.local_clock);
            println!("  Final arena state:");
            for (name, state) in &vm.root_timeline.arena.bindings {
                println!("    {} = {}", name, format_entropic_state(state));
            }
        }
    }

    Ok(())
}
