use crate::frontend::ast::{Expression, Program, Statement, TimeCoordinate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrProgram {
    pub blocks: Vec<IrBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrBlock {
    pub time: TimeCoordinate,
    pub instructions: Vec<IrInstruction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrInstruction {
    pub text: String,
}

impl std::fmt::Display for IrProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for block in &self.blocks {
            writeln!(f, "@{:?}:", block.time)?;
            for instr in &block.instructions {
                writeln!(f, "  {}", instr.text)?;
            }
        }
        Ok(())
    }
}

pub fn lower_program(program: &Program) -> IrProgram {
    let blocks = program
        .timelines
        .iter()
        .map(|t| IrBlock {
            time: t.time.clone(),
            instructions: t
                .statements
                .iter()
                .map(|stmt| IrInstruction {
                    text: lower_statement(&stmt.stmt),
                })
                .collect(),
        })
        .collect();

    IrProgram { blocks }
}

fn lower_statement(stmt: &Statement) -> String {
    match stmt {
        Statement::Isolate(block) => format!(
            "isolate {} {{ ... }}",
            block.name.as_deref().unwrap_or("<anon>")
        ),
        Statement::Split { parent, branches } => {
            format!("split {} into [{:?}]", parent, branches)
        }
        Statement::Merge {
            branches,
            target,
            resolutions,
        } => {
            let rules: Vec<String> = resolutions
                .rules
                .iter()
                .map(|(k, v)| format!("{}={:?}", k, v))
                .collect();
            format!(
                "merge [{:?}] into {} resolving({})",
                branches,
                target,
                rules.join(",")
            )
        }
        Statement::Anchor(name) => format!("anchor {}", name),
        Statement::Rewind(name) => format!("rewind_to {}", name),
        Statement::Commit(_) => "commit { ... }".to_string(),
        Statement::Assignment { target, expr } => {
            format!("{} = {}", target, lower_expression(expr))
        }
        Statement::Send {
            value_id,
            target_branch,
        } => {
            format!("send {} to {}", value_id, target_branch)
        }
        Statement::Expression(expr) => format!("expr {}", lower_expression(expr)),
        Statement::Capability(cap) => format!("require {}", cap.path),
        Statement::ChannelOpen { name, capacity } => {
            format!("open_chan {}({})", name, capacity)
        }
        Statement::ChannelSend { chan_id, value_id } => {
            format!("chan_send {}({})", chan_id, value_id)
        }
        Statement::RelativisticBlock { time, .. } => {
            format!("@{:?} {{ ... }}", time)
        }
        Statement::NetworkRequest { domain } => {
            format!("network_request {}", domain)
        }
        Statement::If {
            condition,
            else_branch,
            ..
        } => {
            let else_txt = if else_branch.is_some() {
                " else { ... }"
            } else {
                ""
            };
            format!("if ({}) {{ ... }}{}", lower_expression(condition), else_txt)
        }
        Statement::Watchdog {
            target, timeout_ms, ..
        } => {
            format!("watchdog {} timeout {}ms", target, timeout_ms)
        }
        Statement::SpeculationMode(mode) => {
            format!("speculation_mode({:?})", mode)
        }
        Statement::For {
            item_name,
            mode,
            source,
            pacing_ms,
            max_ms,
            ..
        } => {
            let pacing_txt = pacing_ms
                .map(|p| format!(" pacing {}ms", p))
                .unwrap_or_default();
            let max_txt = max_ms
                .map(|m| format!(" (max {}ms)", m))
                .unwrap_or_default();
            format!(
                "for {} {} {}{}{} {{ ... }}",
                item_name,
                match mode {
                    crate::frontend::ast::ForMode::Consume => "consume",
                    crate::frontend::ast::ForMode::Clone => "clone",
                },
                source,
                pacing_txt,
                max_txt
            )
        }
        Statement::SplitMap {
            item_name,
            source,
            reconcile,
            ..
        } => {
            let rec_txt = if reconcile.is_some() {
                " reconcile { ... }"
            } else {
                ""
            };
            format!(
                "split_map {} consume {} {{ ... }}{}",
                item_name, source, rec_txt
            )
        }
        Statement::Yield(item) => format!("yield {}", item),
        Statement::RoutineDef {
            name,
            params,
            taking_ms,
            ..
        } => {
            let params_txt: Vec<String> = params
                .iter()
                .map(|(mode, name)| {
                    let mode_str = match mode {
                        crate::frontend::ast::ParamMode::Consume => "consume",
                        crate::frontend::ast::ParamMode::Clone => "clone",
                        crate::frontend::ast::ParamMode::Decay => "decay",
                        crate::frontend::ast::ParamMode::Peek => "peek",
                    };
                    format!("{} {}", mode_str, name)
                })
                .collect();
            format!(
                "routine {}({}) taking {}ms {{ ... }}",
                name,
                params_txt.join(", "),
                taking_ms
            )
        }
        Statement::Loop { max_ms, .. } => {
            format!("loop (max {}ms) {{ ... }}", max_ms)
        }
        Statement::Speculate { max_ms, .. } => {
            format!("speculate (max {}ms) {{ ... }}", max_ms)
        }
        Statement::Select { max_ms, .. } => {
            format!("select (max {}ms) {{ ... }}", max_ms)
        }
        Statement::MatchEntropy { target, .. } => {
            format!("match entropy({}) {{ ... }}", target)
        }
        Statement::Collapse => "collapse".to_string(),
        Statement::Break => "break".to_string(),
        Statement::AcausalReset {
            target,
            anchor_name,
        } => {
            format!("reset {} to {}", target, anchor_name)
        }
    }
}

fn lower_expression(expr: &Expression) -> String {
    match expr {
        Expression::Literal(l) => format!("\"{}\"", l),
        Expression::Identifier(id) => id.clone(),
        Expression::FieldAccess { parent, field } => format!("{}.{}", parent, field),
        Expression::CloneOp(id) => format!("clone({})", id),
        Expression::StructLit(fields) => {
            let members: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{} = {}", k, lower_expression(v)))
                .collect();
            format!("struct {{ {} }}", members.join(", "))
        }
        Expression::ChannelReceive(chan) => format!("chan_recv({})", chan),
        Expression::ArrayLiteral(elements) => {
            let parts: Vec<String> =
                elements.iter().map(|e| lower_expression(e)).collect();
            format!("[{}]", parts.join(","))
        }
        Expression::Integer(i) => format!("{}", i),
        Expression::Call { routine, args } => {
            let args_str: Vec<String> = args.iter().map(|arg| lower_expression(arg)).collect();
            format!("call {}({})", routine, args_str.join(", "))
        }
        Expression::BinaryOp { left, op, right } => {
            let op_str = match op {
                crate::frontend::ast::BinaryOperator::Add => "+",
                crate::frontend::ast::BinaryOperator::Sub => "-",
                crate::frontend::ast::BinaryOperator::Mul => "*",
                crate::frontend::ast::BinaryOperator::Div => "/",
                crate::frontend::ast::BinaryOperator::Eq => "==",
                crate::frontend::ast::BinaryOperator::Neq => "!=",
                crate::frontend::ast::BinaryOperator::Lt => "<",
                crate::frontend::ast::BinaryOperator::Gt => ">",
                crate::frontend::ast::BinaryOperator::Le => "<=",
                crate::frontend::ast::BinaryOperator::Ge => ">=",
            };
            format!(
                "({} {} {})",
                lower_expression(left),
                op_str,
                lower_expression(right)
            )
        }
    }
}
