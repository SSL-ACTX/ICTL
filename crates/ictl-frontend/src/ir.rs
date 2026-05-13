use ictl_core::{Expression, Program, Statement, TimeCoordinate};

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
    pub op: String,
    pub args: Vec<String>,
    pub indent: usize,
}

impl std::fmt::Display for IrProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for block in &self.blocks {
            writeln!(f, "@{}:", block.time)?;
            for instr in &block.instructions {
                let padding = " ".repeat(instr.indent);
                let args = instr.args.join(" ");
                writeln!(f, "{}{} {}", padding, instr.op, args.trim())?;
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
                .flat_map(|stmt| lower_statement_lines(&stmt.stmt, 2))
                .collect(),
        })
        .collect();

    IrProgram { blocks }
}

macro_rules! instr {
    ($indent:expr, $op:expr, $($arg:expr),*) => {
        IrInstruction {
            op: $op.to_string(),
            args: vec![$($arg.to_string()),*],
            indent: $indent,
        }
    };
    ($indent:expr, $op:expr) => {
        IrInstruction {
            op: $op.to_string(),
            args: vec![],
            indent: $indent,
        }
    };
}

fn lower_statement_lines(stmt: &Statement, indent: usize) -> Vec<IrInstruction> {
    match stmt {
        Statement::Isolate(block) => {
            let mut ins = Vec::new();
            let name = block.name.as_deref().unwrap_or("<anon>");
            ins.push(instr!(indent, "ISOLATE", name));
            for inner in &block.body {
                ins.extend(lower_statement_lines(&inner.stmt, indent + 2));
            }
            ins.push(instr!(indent, "END_ISOLATE"));
            ins
        }
        Statement::LoopTick { body } => {
            let mut ins = Vec::new();
            ins.push(instr!(indent, "LOOP_TICK"));
            for inner in body {
                ins.extend(lower_statement_lines(&inner.stmt, indent + 2));
            }
            ins.push(instr!(indent, "END_LOOP_TICK"));
            ins
        }
        Statement::Slice { milliseconds } => {
            vec![instr!(indent, "SLICE", milliseconds)]
        }
        Statement::ChannelOpen { name, capacity } => {
            vec![instr!(indent, "OPEN_CHAN", name, capacity)]
        }
        Statement::ChannelSend { chan_id, value_id } => {
            vec![instr!(indent, "CHAN_SEND", chan_id, value_id)]
        }
        Statement::Assignment { target, expr, .. } => {
            vec![instr!(indent, "ASSIGN", target, lower_expression(expr))]
        }
        Statement::Break => vec![instr!(indent, "BREAK")],
        Statement::Print(expr) => {
            vec![instr!(indent, "PRINT", lower_expression(expr))]
        }
        Statement::Await(target) => vec![instr!(indent, "AWAIT", target)],
        Statement::If { condition, .. } => {
            vec![instr!(indent, "IF", lower_expression(condition))]
        }
        Statement::Select { max_ms, .. } => vec![instr!(indent, "SELECT", max_ms)],
        Statement::Entangle { variables } => {
            let mut args = Vec::new();
            for var in variables {
                args.push(var.clone());
            }
            vec![IrInstruction {
                op: "ENTANGLE".to_string(),
                args,
                indent,
            }]
        }
        Statement::TypeDecl { name, fields, .. } => {
            let mut args = Vec::new();
            for key in fields.keys() {
                args.push(key.clone());
            }
            vec![IrInstruction {
                op: format!("TYPE_DECL {}", name),
                args,
                indent,
            }]
        }
        Statement::DecayHandler { type_name, .. } => {
            vec![instr!(indent, format!("DECAY_HANDLER {}", type_name))]
        }
        Statement::AssertTime {
            operator, limit_ms, ..
        } => vec![instr!(
            indent,
            "ASSERT_TIME",
            format!("{:?}", operator),
            limit_ms
        )],
        _ => vec![instr!(indent, lower_statement(stmt))],
    }
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
        Statement::Assignment { target, expr, .. } => {
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
        Statement::Print(expr) => format!("print({})", lower_expression(expr)),
        Statement::Debug(expr) => format!("debug({})", lower_expression(expr)),
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
                    ictl_core::ForMode::Consume => "consume",
                    ictl_core::ForMode::Clone => "clone",
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
            return_type,
            taking_ms,
            ..
        } => {
            fn type_name_to_string(t: &ictl_core::TypeName) -> String {
                match t {
                    ictl_core::TypeName::Builtin(b) => match b {
                        ictl_core::BuiltinType::Integer => "int".to_string(),
                        ictl_core::BuiltinType::Bool => "bool".to_string(),
                        ictl_core::BuiltinType::String => "string".to_string(),
                        ictl_core::BuiltinType::Struct => "struct".to_string(),
                        ictl_core::BuiltinType::Topology => "topology".to_string(),
                        ictl_core::BuiltinType::Array => "array".to_string(),
                    },
                    ictl_core::TypeName::Custom(name) => name.clone(),
                    ictl_core::TypeName::Optional(inner) => {
                        format!("{}?", type_name_to_string(inner))
                    }
                    ictl_core::TypeName::Union(parts) => parts
                        .iter()
                        .map(type_name_to_string)
                        .collect::<Vec<_>>()
                        .join("|"),
                }
            }

            let params_txt: Vec<String> = params
                .iter()
                .map(|param| {
                    let mode_str = match param.mode {
                        ictl_core::ParamMode::Consume => "consume",
                        ictl_core::ParamMode::Clone => "clone",
                        ictl_core::ParamMode::Decay => "decay",
                        ictl_core::ParamMode::Peek => "peek",
                    };
                    let type_str = param
                        .typ
                        .as_ref()
                        .map(type_name_to_string)
                        .unwrap_or_default();
                    if type_str.is_empty() {
                        format!("{} {}", mode_str, param.name)
                    } else {
                        format!("{} {}:{}", mode_str, param.name, type_str)
                    }
                })
                .collect();
            let taking_display = taking_ms.unwrap_or(0);
            let return_txt = return_type
                .as_ref()
                .map(|t| format!(" -> {}", type_name_to_string(t)))
                .unwrap_or_default();
            format!(
                "routine {}({}){} taking {}ms {{ ... }}",
                name,
                params_txt.join(", "),
                return_txt,
                taking_display
            )
        }
        Statement::Inspect { target, .. } => {
            format!("inspect({}) {{ ... }}", target)
        }
        Statement::Loop { max_ms, .. } => {
            format!("loop (max {}ms) {{ ... }}", max_ms)
        }
        Statement::LoopTick { .. } => "loop tick { ... }".to_string(),
        Statement::Slice { milliseconds } => {
            format!("slice {}ms", milliseconds)
        }
        Statement::Speculate { max_ms, .. } => {
            format!("speculate (max {}ms) {{ ... }}", max_ms)
        }
        Statement::Select { max_ms, .. } => {
            format!("select (max {}ms) {{ ... }}", max_ms)
        }
        Statement::MatchEntropy { target, .. } => {
            format!("match entropy({}) {{ ... }}", lower_expression(target))
        }
        Statement::Await(target) => format!("await({})", target),
        Statement::Collapse => "collapse".to_string(),
        Statement::Break => "break".to_string(),
        Statement::AcausalReset {
            target,
            anchor_name,
        } => {
            format!("reset {} to {}", target, anchor_name)
        }
        Statement::FieldUpdate {
            target,
            field,
            value,
        } => {
            format!(
                "{}.{} = {}",
                lower_expression(target),
                field,
                lower_expression(value)
            )
        }
        Statement::TypeDecl { name, .. } => format!("type {} {{ ... }}", name),
        Statement::DecayHandler { type_name, .. } => {
            format!("decay_handler for {} {{ ... }}", type_name)
        }
        Statement::AssertTime {
            operator, limit_ms, ..
        } => {
            format!(
                "assert_time (elapsed {:?} {}ms) {{ ... }}",
                operator, limit_ms
            )
        }
        Statement::Entangle { variables } => {
            format!("entangle({})", variables.join(", "))
        }
    }
}

fn lower_expression(expr: &Expression) -> String {
    match expr {
        Expression::Literal(l) => format!("\"{}\"", l),
        Expression::Identifier(id) => id.clone(),
        Expression::Null => "null".to_string(),
        Expression::FieldAccess { target, field } => {
            format!("{}.{}", lower_expression(target), field)
        }
        Expression::CloneOp(id) => format!("clone({})", id),
        Expression::StructLit(fields) | Expression::TopologyLit(fields) => {
            let members: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{} = {}", k, lower_expression(v)))
                .collect();
            let name = match expr {
                Expression::StructLit(_) => "struct",
                Expression::TopologyLit(_) => "topology",
                _ => "unreachable",
            };
            format!("{} {{ {} }}", name, members.join(", "))
        }
        Expression::IndexAccess { target, index } => {
            format!("{}[{}]", lower_expression(target), lower_expression(index))
        }
        Expression::ChannelReceive(chan) => format!("chan_recv({})", chan),
        Expression::ArrayLiteral(elements) => {
            let parts: Vec<String> = elements.iter().map(lower_expression).collect();
            format!("[{}]", parts.join(","))
        }
        Expression::Integer(i) => format!("{}", i),
        Expression::Boolean(b) => format!("{}", b),
        Expression::Deferred { capability, .. } => {
            format!("defer {}(...)", capability)
        }
        Expression::Call { routine, args } => {
            let args_str: Vec<String> = args.iter().map(lower_expression).collect();
            format!("call {}({})", routine, args_str.join(", "))
        }
        Expression::BinaryOp { left, op, right } => {
            let op_str = match op {
                ictl_core::BinaryOperator::Add => "+",
                ictl_core::BinaryOperator::Sub => "-",
                ictl_core::BinaryOperator::Mul => "*",
                ictl_core::BinaryOperator::Div => "/",
                ictl_core::BinaryOperator::Eq => "==",
                ictl_core::BinaryOperator::Neq => "!=",
                ictl_core::BinaryOperator::Lt => "<",
                ictl_core::BinaryOperator::Gt => ">",
                ictl_core::BinaryOperator::Le => "<=",
                ictl_core::BinaryOperator::Ge => ">=",
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
