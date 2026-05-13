use ictl_analysis::analyzer::EntropicAnalyzer;
use ictl_core::value::EntropicState;
use ictl_frontend::ir;
use ictl_frontend::parser;
use ictl_runtime::vm::Vm;
use std::env;
use std::fs;
use std::path::PathBuf;

fn usage(program: &str) {
    eprintln!(
        "Usage: {} [--check] [--run] [--dump-ast] [--dump-ir] [--trace-entropy] <file1.ictl> [file2.ictl ...]",
        program
    );
    eprintln!("  --check          Perform semantic analysis only");
    eprintln!("  --run            Execute program after analysis (default)");
    eprintln!("  --dump-ast       Print the parsed AST and continue");
    eprintln!("  --dump-ir        Print the lowered IR and continue");
    eprintln!("  --trace-entropy  Show entropic decay map after every instruction");
}

fn format_entropic_state(state: &EntropicState) -> String {
    match state {
        EntropicState::Valid(p) => format!("{}", p),
        EntropicState::Decayed(_) => "<decayed>".to_string(),
        EntropicState::Pending(_) => "<pending>".to_string(),
        EntropicState::Consumed => "Consumed".to_string(),
    }
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
    let mut trace_entropy = false;

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
        if arg == "--trace-entropy" {
            trace_entropy = true;
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

        let program = match parser::parse_ictl(&source) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "error: failed to parse {}\n  --> {}\n      {}",
                    path.display(),
                    path.display(),
                    e
                );
                continue;
            }
        };

        if dump_ast {
            println!("AST for {}:\n{:#?}", path.display(), program);
        }

        if dump_ir {
            let ir_program = ir::lower_program(&program);
            println!("IR for {}:\n{}", path.display(), ir_program);
        }

        let mut analyzer = EntropicAnalyzer::new();
        if let Err(err) = analyzer.analyze_program_with_source(
            &program,
            &source,
            &path.display().to_string(),
        ) {
            let formatted = analyzer.format_semantic_error(&err);
            eprintln!("error: {}", formatted);
            continue;
        }

        println!("\x1b[1;32m{}: analysis ok\x1b[0m", path.display());

        if run_program {
            let ir_program = ir::lower_program(&program);
            let mut vm = Vm::new();
            vm.trace_entropy = trace_entropy;
            vm.register_capability("System.Log", |params| {
                if let Some(msg) = params.get("message") {
                    println!("\x1b[1;34m[System.Log]\x1b[0m {}", msg);
                }
                Ok(())
            });

            if let Err(e) = vm.execute_program(&ir_program) {
                eprintln!(
                    "\x1b[1;31merror: runtime failure in {}\x1b[0m\n  cause: {}",
                    path.display(),
                    e
                );
            }

            println!("\x1b[1;32m{}: run ok\x1b[0m", path.display());
            println!("\x1b[1;36m┌─ Execution Summary ──┐\x1b[0m");
            println!("\x1b[1;36m│\x1b[0m Global clock:    {}", vm.global_clock);
            println!(
                "\x1b[1;36m│\x1b[0m Main local clock: {}",
                vm.root_timeline.local_clock
            );
            println!(
                "\x1b[1;36m│\x1b[0m Arena memory:    {}/{} bytes used",
                vm.root_timeline.arena.used, vm.root_timeline.arena.capacity
            );
            println!("\x1b[1;36m└──────────────────────┘\x1b[0m");
            println!("\x1b[1;35mFinal Arena State:\x1b[0m");
            for (i, state) in vm.root_timeline.arena.registers.iter().enumerate() {
                if !matches!(state, EntropicState::Consumed) {
                    println!(
                        "  \x1b[1;33mR{: <10}\x1b[0m = {}",
                        i,
                        format_entropic_state(state)
                    );
                }
            }
        }
    }

    Ok(())
}
