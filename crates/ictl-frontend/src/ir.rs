use ictl_core::{BinaryOperator, Expression, Program, Statement, TimeCoordinate};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg(pub u32);

impl std::fmt::Display for Reg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "R{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrSelectCase {
    pub chan_id: String,
    pub dest: Reg,
    pub target: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    // Arithmetic & Logic
    BinaryOp {
        dest: Reg,
        op: BinaryOperator,
        left: Reg,
        right: Reg,
    },

    // Data Movement
    LoadInt {
        dest: Reg,
        value: i64,
    },
    LoadBool {
        dest: Reg,
        value: bool,
    },
    LoadString {
        dest: Reg,
        value: String,
    },
    LoadNull {
        dest: Reg,
    },
    Move {
        dest: Reg,
        src: Reg,
    },

    // Entropic Operations
    Consume {
        src: Reg,
    },
    ConsumeField {
        src: Reg,
        field: String,
    },
    ConsumeFieldDynamic {
        target: Reg,
        index: Reg,
    },
    Clone {
        dest: Reg,
        src: Reg,
    },

    // Control Flow
    Jump {
        target: usize,
    },
    JumpIf {
        cond: Reg,
        target: usize,
    },
    JumpIfNot {
        cond: Reg,
        target: usize,
    },
    Call {
        routine: String,
        args: Vec<Reg>,
        dest: Reg,
    },
    Return {
        src: Option<Reg>,
    },

    // ICTL Temporal & Isolated Concurrency
    Isolate {
        name: String,
        manifest: ictl_core::Manifest,
    },
    EndIsolate,
    Split {
        parent: String,
        branches: Vec<String>,
    },
    Merge {
        branches: Vec<String>,
        target: String,
        resolution: ictl_core::MergeResolution,
    },
    Entangle {
        regs: Vec<Reg>,
    },
    Anchor {
        name: String,
    },
    Rewind {
        target: String,
        anchor: String,
    },
    Commit {
        vars: Vec<String>,
    }, // Variables to commit back
    Watchdog {
        target: String,
        timeout_ms: u64,
        recovery_jump: Option<usize>,
    },
    Speculate {
        max_ms: u64,
        fallback_target: usize,
    },
    EndSpeculate {
        max_ms: u64,
        fallback_target: usize,
    },
    Collapse,
    Select {
        max_ms: u64,
        cases: Vec<IrSelectCase>,
        timeout_target: Option<usize>,
    },
    MatchEntropy {
        target: Reg,
        valid_target: Option<usize>,
        decayed_target: Option<usize>,
        pending_target: Option<usize>,
        consumed_target: Option<usize>,
    },
    RelativisticBlock {
        target: String,
        block_pc: usize,
        block_len: usize,
    },
    SpeculationMode {
        mode: ictl_core::SpeculationCommitMode,
    },

    // Channels & Communication
    OpenChan {
        name: String,
        capacity: usize,
    },
    ChanSend {
        chan_id: String,
        src: Reg,
    },
    ChanRecv {
        dest: Reg,
        chan_id: String,
    },

    // Structural Access
    StructLit {
        dest: Reg,
        fields: HashMap<String, Reg>,
        type_name: Option<String>,
    },
    TopologyLit {
        dest: Reg,
        fields: HashMap<String, Reg>,
    },
    ArrayLit {
        dest: Reg,
        elements: Vec<Reg>,
    },
    FieldAccess {
        dest: Reg,
        target: Reg,
        field: String,
    },
    FieldUpdate {
        target: Reg,
        field: String,
        src: Reg,
    },
    IndexAccess {
        dest: Reg,
        target: Reg,
        index: Reg,
    },
    IndexFieldUpdate {
        target: Reg,
        index: Reg,
        field: String,
        src: Reg,
    },
    // Misc
    Print {
        src: Reg,
    },
    Debug {
        src: Reg,
    },
    AssertTime {
        op: BinaryOperator,
        limit_ms: u64,
    },
    Slice {
        ms: u64,
    },
    Break,
    LoopTick,
    EndLoopTick,
    Capability {
        cap: ictl_core::Capability,
    },
    For {
        item_name: String,
        mode: ictl_core::ForMode,
        source: Reg,
        body: Vec<Instruction>,
        pacing_ms: Option<u64>,
        max_ms: Option<u64>,
    },
    SplitMap {
        item_name: String,
        mode: ictl_core::ForMode,
        source: Reg,
        body: Vec<Instruction>,
        reconcile: Option<ictl_core::MergeResolution>,
    },
    Defer {
        dest: Reg,
        cap: ictl_core::Capability,
        deadline_ms: u64,
    },
    Await {
        target: Reg,
    },
    Loop {
        max_ms: u64,
    },
    EndLoop {
        max_ms: u64,
    },
    NetworkRequest {
        domain: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrProgram {
    pub blocks: Vec<IrBlock>,
    pub routines: HashMap<String, IrRoutine>,
    pub symbols: HashMap<String, Reg>, // Map names to registers for debugging/tests
    pub type_decay_limits: HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrRoutine {
    pub params: Vec<(ictl_core::ParamMode, String, ictl_core::types::Type)>,
    pub return_type: ictl_core::types::Type,
    pub taking_ms: Option<u64>,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrBlock {
    pub time: TimeCoordinate,
    pub instructions: Vec<Instruction>,
}

impl std::fmt::Display for IrProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, routine) in &self.routines {
            writeln!(f, "routine {} taking {:?}ms:", name, routine.taking_ms)?;
            for (i, instr) in routine.instructions.iter().enumerate() {
                writeln!(f, "  {:>3}: {:?}", i, instr)?;
            }
        }
        for block in &self.blocks {
            writeln!(f, "@{}:", block.time)?;
            for (i, instr) in block.instructions.iter().enumerate() {
                writeln!(f, "  {:>3}: {:?}", i, instr)?;
            }
        }
        Ok(())
    }
}

struct LoweringContext {
    next_reg: u32,
    symbols: HashMap<String, Reg>,
    instructions: Vec<Instruction>,
    routines: HashMap<String, IrRoutine>,
    type_decay_limits: HashMap<String, u64>,
}

impl LoweringContext {
    fn new() -> Self {
        Self {
            next_reg: 0,
            symbols: HashMap::new(),
            instructions: Vec::new(),
            routines: HashMap::new(),
            type_decay_limits: HashMap::new(),
        }
    }

    fn alloc_reg(&mut self) -> Reg {
        let r = Reg(self.next_reg);
        self.next_reg += 1;
        r
    }

    fn get_reg(&mut self, name: &str) -> Reg {
        if let Some(r) = self.symbols.get(name) {
            *r
        } else {
            let r = self.alloc_reg();
            self.symbols.insert(name.to_string(), r);
            r
        }
    }

    fn push(&mut self, instr: Instruction) {
        self.instructions.push(instr);
    }
}

pub fn lower_program(program: &Program) -> IrProgram {
    let mut blocks = Vec::new();
    let mut ctx = LoweringContext::new();

    for tb in &program.timelines {
        let start_idx = ctx.instructions.len();
        for stmt in &tb.statements {
            lower_statement(&mut ctx, &stmt.stmt);
        }
        let block_instrs = ctx.instructions.split_off(start_idx);
        blocks.push(IrBlock {
            time: tb.time.clone(),
            instructions: block_instrs,
        });
    }

    IrProgram {
        blocks,
        routines: ctx.routines,
        symbols: ctx.symbols,
        type_decay_limits: ctx.type_decay_limits,
    }
}

fn lower_statement(ctx: &mut LoweringContext, stmt: &Statement) {
    match stmt {
        Statement::RoutineDef {
            name,
            params,
            return_type,
            taking_ms,
            body,
        } => {
            let mut sub_ctx = LoweringContext::new();

            // Map parameters to registers R0, R1, ...
            for (i, param) in params.iter().enumerate() {
                sub_ctx.symbols.insert(param.name.clone(), Reg(i as u32));
                sub_ctx.next_reg = (i + 1) as u32;
            }

            for s in body {
                lower_statement(&mut sub_ctx, &s.stmt);
            }

            let routine = IrRoutine {
                params: params
                    .iter()
                    .map(|p| {
                        (
                            p.mode.clone(),
                            p.name.clone(),
                            p.typ
                                .as_ref()
                                .map(ictl_core::types::Type::from_typename)
                                .unwrap_or(ictl_core::types::Type::Unknown),
                        )
                    })
                    .collect(),
                return_type: return_type
                    .as_ref()
                    .map(ictl_core::types::Type::from_typename)
                    .unwrap_or(ictl_core::types::Type::Unknown),
                taking_ms: *taking_ms,
                instructions: sub_ctx.instructions,
            };
            ctx.routines.insert(name.clone(), routine);
        }
        Statement::Yield(name) => {
            let src = ctx.get_reg(name);
            // By convention, Move src to R0 for return
            ctx.push(Instruction::Move { dest: Reg(0), src });
            ctx.push(Instruction::Return { src: Some(Reg(0)) });
        }
        Statement::Speculate {
            max_ms,
            body,
            fallback,
        } => {
            let spec_idx = ctx.instructions.len();
            ctx.push(Instruction::Speculate {
                max_ms: *max_ms,
                fallback_target: 0, // Placeholder
            });

            for s in body {
                lower_statement(ctx, &s.stmt);
            }

            let end_spec_idx = ctx.instructions.len();
            ctx.push(Instruction::EndSpeculate {
                max_ms: *max_ms,
                fallback_target: 0, // Placeholder
            });

            let jump_over_fallback_idx = ctx.instructions.len();
            ctx.push(Instruction::Jump { target: 0 }); // Placeholder

            let fallback_start_idx = ctx.instructions.len();
            if let Instruction::Speculate {
                ref mut fallback_target,
                ..
            } = ctx.instructions[spec_idx]
            {
                *fallback_target = fallback_start_idx;
            }
            if let Instruction::EndSpeculate {
                ref mut fallback_target,
                ..
            } = ctx.instructions[end_spec_idx]
            {
                *fallback_target = fallback_start_idx;
            }

            if let Some(fb) = fallback {
                for s in fb {
                    lower_statement(ctx, &s.stmt);
                }
            }

            let end_idx = ctx.instructions.len();
            if let Instruction::Jump { ref mut target, .. } =
                ctx.instructions[jump_over_fallback_idx]
            {
                *target = end_idx;
            }
        }
        Statement::Collapse => {
            ctx.push(Instruction::Collapse);
        }
        Statement::SpeculationMode(mode) => {
            ctx.push(Instruction::SpeculationMode { mode: *mode });
        }
        Statement::Select {
            max_ms,
            cases,
            timeout,
            ..
        } => {
            let select_idx = ctx.instructions.len();
            ctx.push(Instruction::Select {
                max_ms: *max_ms,
                cases: Vec::new(), // Will fill in below
                timeout_target: None,
            });

            let mut ir_cases = Vec::new();
            let mut case_jumps = Vec::new();

            for case in cases {
                let chan_id = match &case.source {
                    Expression::ChannelReceive(id) => id.clone(),
                    _ => "".to_string(), // Error handling?
                };
                let dest = ctx.get_reg(&case.binding);
                let target = ctx.instructions.len();

                for s in &case.body {
                    lower_statement(ctx, &s.stmt);
                }
                case_jumps.push(ctx.instructions.len());
                ctx.push(Instruction::Jump { target: 0 }); // Jump to end of select

                ir_cases.push(IrSelectCase {
                    chan_id,
                    dest,
                    target,
                });
            }

            if let Instruction::Select { ref mut cases, .. } =
                ctx.instructions[select_idx]
            {
                *cases = ir_cases;
            }

            if let Some(t) = timeout {
                let timeout_start = ctx.instructions.len();
                if let Instruction::Select {
                    ref mut timeout_target,
                    ..
                } = ctx.instructions[select_idx]
                {
                    *timeout_target = Some(timeout_start);
                }
                for s in t {
                    lower_statement(ctx, &s.stmt);
                }
            }

            let end_idx = ctx.instructions.len();
            for jump_idx in case_jumps {
                if let Instruction::Jump { ref mut target, .. } =
                    ctx.instructions[jump_idx]
                {
                    *target = end_idx;
                }
            }
        }
        Statement::RelativisticBlock { time, body } => {
            let target = match time {
                ictl_core::TimeCoordinate::Branch(b) => b.clone(),
                _ => "main".to_string(),
            };

            let jump_over_idx = ctx.instructions.len();
            ctx.push(Instruction::Jump { target: 0 }); // Jump over body

            let start_pc = ctx.instructions.len();
            for s in body {
                lower_statement(ctx, &s.stmt);
            }
            let len = ctx.instructions.len() - start_pc;

            let end_pc = ctx.instructions.len();
            if let Instruction::Jump { ref mut target, .. } =
                ctx.instructions[jump_over_idx]
            {
                *target = end_pc;
            }

            ctx.push(Instruction::RelativisticBlock {
                target,
                block_pc: start_pc,
                block_len: len,
            });
        }
        Statement::MatchEntropy {
            target,
            valid_branch,
            decayed_branch,
            pending_branch,
            consumed_branch,
        } => {
            let target_reg = lower_expression(ctx, target);
            let match_idx = ctx.instructions.len();
            ctx.push(Instruction::MatchEntropy {
                target: target_reg,
                valid_target: None,
                decayed_target: None,
                pending_target: None,
                consumed_target: None,
            });

            let mut branch_jumps = Vec::new();

            if let Some((binding, body)) = valid_branch {
                let start = ctx.instructions.len();
                if let Instruction::MatchEntropy {
                    ref mut valid_target,
                    ..
                } = ctx.instructions[match_idx]
                {
                    *valid_target = Some(start);
                }

                let dest = ctx.get_reg(binding);
                ctx.push(Instruction::Move {
                    dest,
                    src: target_reg,
                });

                for s in body {
                    lower_statement(ctx, &s.stmt);
                }
                branch_jumps.push(ctx.instructions.len());
                ctx.push(Instruction::Jump { target: 0 });
            }

            if let Some((binding, body)) = decayed_branch {
                let start = ctx.instructions.len();
                if let Instruction::MatchEntropy {
                    ref mut decayed_target,
                    ..
                } = ctx.instructions[match_idx]
                {
                    *decayed_target = Some(start);
                }

                let dest = ctx.get_reg(binding);
                ctx.push(Instruction::Move {
                    dest,
                    src: target_reg,
                });

                for s in body {
                    lower_statement(ctx, &s.stmt);
                }
                branch_jumps.push(ctx.instructions.len());
                ctx.push(Instruction::Jump { target: 0 });
            }

            if let Some(body) = pending_branch {
                let start = ctx.instructions.len();
                if let Instruction::MatchEntropy {
                    ref mut pending_target,
                    ..
                } = ctx.instructions[match_idx]
                {
                    *pending_target = Some(start);
                }
                for s in body {
                    lower_statement(ctx, &s.stmt);
                }
                branch_jumps.push(ctx.instructions.len());
                ctx.push(Instruction::Jump { target: 0 });
            }

            if let Some(body) = consumed_branch {
                let start = ctx.instructions.len();
                if let Instruction::MatchEntropy {
                    ref mut consumed_target,
                    ..
                } = ctx.instructions[match_idx]
                {
                    *consumed_target = Some(start);
                }
                for s in body {
                    lower_statement(ctx, &s.stmt);
                }
                branch_jumps.push(ctx.instructions.len());
                ctx.push(Instruction::Jump { target: 0 });
            }

            let end_idx = ctx.instructions.len();
            for jump_idx in branch_jumps {
                if let Instruction::Jump { ref mut target, .. } =
                    ctx.instructions[jump_idx]
                {
                    *target = end_idx;
                }
            }
        }
        Statement::Assignment { target, expr, .. } => {
            let src = lower_expression(ctx, expr);
            let dest = ctx.get_reg(target);
            ctx.push(Instruction::Move { dest, src });

            // Consuming move by default in ICTL
            match expr {
                Expression::Identifier(_) => {
                    ctx.push(Instruction::Consume { src });
                }
                Expression::IndexAccess {
                    target: inner_target,
                    index,
                } => {
                    let graph_reg = lower_expression(ctx, inner_target);
                    let index_reg = lower_expression(ctx, index);
                    // We need to know the field name/index at runtime.
                    // For now, let's assume it's a string index.
                    // Instruction::ConsumeIndex { target, index }
                    ctx.push(Instruction::ConsumeFieldDynamic {
                        target: graph_reg,
                        index: index_reg,
                    });
                }
                _ => {}
            }
        }
        Statement::Print(expr) => {
            let src = lower_expression(ctx, expr);
            ctx.push(Instruction::Print { src });
        }
        Statement::Debug(expr) => {
            let src = lower_expression(ctx, expr);
            ctx.push(Instruction::Debug { src });
        }
        Statement::Isolate(block) => {
            let name = block.name.clone().unwrap_or_else(|| "<anon>".to_string());
            ctx.push(Instruction::Isolate {
                name,
                manifest: block.manifest.clone(),
            });
            for s in &block.body {
                lower_statement(ctx, &s.stmt);
            }
            ctx.push(Instruction::EndIsolate);
        }
        Statement::Capability(cap) => {
            ctx.push(Instruction::Capability { cap: cap.clone() });
        }
        Statement::For {
            item_name,
            mode,
            source,
            body,
            pacing_ms,
            max_ms,
        } => {
            let source_reg = ctx.get_reg(source);
            let mut sub_ctx = LoweringContext::new();
            sub_ctx.symbols = ctx.symbols.clone();
            sub_ctx.next_reg = ctx.next_reg;

            // Item name is in a register
            let _ = sub_ctx.get_reg(item_name);

            for s in body {
                lower_statement(&mut sub_ctx, &s.stmt);
            }

            ctx.symbols = sub_ctx.symbols;
            ctx.next_reg = sub_ctx.next_reg;

            ctx.push(Instruction::For {
                item_name: item_name.clone(),
                mode: mode.clone(),
                source: source_reg,
                body: sub_ctx.instructions,
                pacing_ms: *pacing_ms,
                max_ms: *max_ms,
            });
        }
        Statement::SplitMap {
            item_name,
            mode,
            source,
            body,
            reconcile,
        } => {
            let source_reg = ctx.get_reg(source);
            // Ensure splitmap_results is reserved in the parent context
            let _ = ctx.get_reg("splitmap_results");

            let mut sub_ctx = LoweringContext::new();
            sub_ctx.symbols = ctx.symbols.clone();
            sub_ctx.next_reg = ctx.next_reg;

            // Item name is in a register
            let _ = sub_ctx.get_reg(item_name);

            for s in body {
                lower_statement(&mut sub_ctx, &s.stmt);
            }

            ctx.symbols = sub_ctx.symbols;
            ctx.next_reg = sub_ctx.next_reg;

            ctx.push(Instruction::SplitMap {
                item_name: item_name.clone(),
                mode: mode.clone(),
                source: source_reg,
                body: sub_ctx.instructions,
                reconcile: reconcile.clone(),
            });
        }
        Statement::Split { parent, branches } => {
            ctx.push(Instruction::Split {
                parent: parent.clone(),
                branches: branches.clone(),
            });
        }
        Statement::Merge {
            branches,
            target,
            resolutions,
            ..
        } => {
            ctx.push(Instruction::Merge {
                branches: branches.clone(),
                target: target.clone(),
                resolution: resolutions.clone(),
            });
        }
        Statement::Anchor(name) => {
            ctx.push(Instruction::Anchor { name: name.clone() });
        }
        Statement::Rewind(name) => {
            // Acausal reset/rewind
            ctx.push(Instruction::Rewind {
                target: "self".to_string(), // Default to current branch for rewind
                anchor: name.clone(),
            });
        }
        Statement::AcausalReset {
            target,
            anchor_name,
        } => {
            ctx.push(Instruction::Rewind {
                target: target.clone(),
                anchor: anchor_name.clone(),
            });
        }
        Statement::NetworkRequest { domain } => {
            ctx.push(Instruction::NetworkRequest {
                domain: domain.clone(),
            });
        }
        Statement::Entangle { variables } => {
            let regs = variables.iter().map(|v| ctx.get_reg(v)).collect();
            ctx.push(Instruction::Entangle { regs });
        }
        Statement::Await(name) => {
            let target = ctx.get_reg(name);
            ctx.push(Instruction::Await { target });
        }
        Statement::Commit(body) => {
            // Simplified commit: we collect modified vars.
            // In a real compiler we'd track what's assigned in the body.
            // For now, let's just lower the body and use a placeholder for vars.
            for s in body {
                lower_statement(ctx, &s.stmt);
            }
            ctx.push(Instruction::Commit { vars: Vec::new() });
        }

        Statement::ChannelOpen { name, capacity } => {
            ctx.push(Instruction::OpenChan {
                name: name.clone(),
                capacity: *capacity,
            });
        }
        Statement::ChannelSend { chan_id, value_id } => {
            let src = ctx.get_reg(value_id);
            ctx.push(Instruction::ChanSend {
                chan_id: chan_id.clone(),
                src,
            });
        }
        Statement::Slice { milliseconds } => {
            ctx.push(Instruction::Slice { ms: *milliseconds });
        }
        Statement::Break => {
            ctx.push(Instruction::Break);
        }
        Statement::LoopTick { body } => {
            ctx.push(Instruction::LoopTick);
            for s in body {
                lower_statement(ctx, &s.stmt);
            }
            ctx.push(Instruction::EndLoopTick);
        }
        Statement::Loop { max_ms, body } => {
            ctx.push(Instruction::Loop { max_ms: *max_ms });
            for s in body {
                lower_statement(ctx, &s.stmt);
            }
            ctx.push(Instruction::EndLoop { max_ms: *max_ms });
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let cond_reg = lower_expression(ctx, condition);

            let jump_to_else_idx = ctx.instructions.len();
            ctx.push(Instruction::JumpIfNot {
                cond: cond_reg,
                target: 0,
            }); // Placeholder

            for s in then_branch {
                lower_statement(ctx, &s.stmt);
            }

            if let Some(eb) = else_branch {
                let jump_to_end_idx = ctx.instructions.len();
                ctx.push(Instruction::Jump { target: 0 }); // Placeholder

                let else_start_idx = ctx.instructions.len();
                if let Instruction::JumpIfNot { ref mut target, .. } =
                    ctx.instructions[jump_to_else_idx]
                {
                    *target = else_start_idx;
                }

                for s in eb {
                    lower_statement(ctx, &s.stmt);
                }

                let end_idx = ctx.instructions.len();
                if let Instruction::Jump { ref mut target, .. } =
                    ctx.instructions[jump_to_end_idx]
                {
                    *target = end_idx;
                }
            } else {
                let end_idx = ctx.instructions.len();
                if let Instruction::JumpIfNot { ref mut target, .. } =
                    ctx.instructions[jump_to_else_idx]
                {
                    *target = end_idx;
                }
            }
        }
        Statement::Watchdog {
            target,
            timeout_ms,
            recovery,
        } => {
            // For watchdog, recovery is a block. We use Jump for simplicity in this flat IR.
            let jump_over_recovery_idx = ctx.instructions.len();
            ctx.push(Instruction::Watchdog {
                target: target.clone(),
                timeout_ms: *timeout_ms,
                recovery_jump: Some(0), // Placeholder
            });

            // Jump over recovery by default if watchdog doesn't bite
            let skip_recovery_idx = ctx.instructions.len();
            ctx.push(Instruction::Jump { target: 0 }); // Placeholder

            let recovery_start_idx = ctx.instructions.len();
            if let Instruction::Watchdog {
                ref mut recovery_jump,
                ..
            } = ctx.instructions[jump_over_recovery_idx]
            {
                *recovery_jump = Some(recovery_start_idx);
            }

            for s in recovery {
                lower_statement(ctx, &s.stmt);
            }

            let end_idx = ctx.instructions.len();
            if let Instruction::Jump { ref mut target, .. } =
                ctx.instructions[skip_recovery_idx]
            {
                *target = end_idx;
            }
        }
        Statement::FieldUpdate {
            target,
            field,
            value,
        } => match target {
            Expression::Identifier(name) => {
                let target_reg = ctx.get_reg(name);
                let src_reg = lower_expression(ctx, value);
                ctx.push(Instruction::FieldUpdate {
                    target: target_reg,
                    field: field.clone(),
                    src: src_reg,
                });
            }
            Expression::IndexAccess {
                target: inner_target,
                index,
            } => {
                let graph_reg = lower_expression(ctx, inner_target);
                let index_reg = lower_expression(ctx, index);
                let src_reg = lower_expression(ctx, value);
                ctx.push(Instruction::IndexFieldUpdate {
                    target: graph_reg,
                    index: index_reg,
                    field: field.clone(),
                    src: src_reg,
                });
            }
            _ => {
                let target_reg = lower_expression(ctx, target);
                let src_reg = lower_expression(ctx, value);
                ctx.push(Instruction::FieldUpdate {
                    target: target_reg,
                    field: field.clone(),
                    src: src_reg,
                });
            }
        },
        Statement::Expression(expr) => {
            lower_expression(ctx, expr);
        }
        Statement::AssertTime {
            operator, limit_ms, ..
        } => {
            ctx.push(Instruction::AssertTime {
                op: *operator,
                limit_ms: *limit_ms,
            });
        }
        Statement::TypeDecl {
            name,
            decay_after_ms: Some(limit),
            ..
        } => {
            ctx.type_decay_limits.insert(name.clone(), *limit);
        }
        Statement::TypeDecl { .. } => {}
        _ => {
            // Other statements can be added as needed
        }
    }
}

fn lower_expression(ctx: &mut LoweringContext, expr: &Expression) -> Reg {
    match expr {
        Expression::Integer(i) => {
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::LoadInt { dest, value: *i });
            dest
        }
        Expression::Boolean(b) => {
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::LoadBool { dest, value: *b });
            dest
        }
        Expression::Literal(s) => {
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::LoadString {
                dest,
                value: s.clone(),
            });
            dest
        }
        Expression::Null => {
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::LoadNull { dest });
            dest
        }
        Expression::Identifier(name) => ctx.get_reg(name),
        Expression::BinaryOp { left, op, right } => {
            let l = lower_expression(ctx, left);
            let r = lower_expression(ctx, right);
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::BinaryOp {
                dest,
                op: *op,
                left: l,
                right: r,
            });
            dest
        }
        Expression::Call { routine, args } => {
            let mut arg_regs = Vec::new();
            for arg in args {
                arg_regs.push(lower_expression(ctx, arg));
            }
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::Call {
                routine: routine.clone(),
                args: arg_regs,
                dest,
            });
            dest
        }
        Expression::CloneOp(name) => {
            let src = ctx.get_reg(name);
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::Clone { dest, src });
            dest
        }
        Expression::FieldAccess { target, field } => {
            let t = lower_expression(ctx, target);
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::FieldAccess {
                dest,
                target: t,
                field: field.clone(),
            });
            dest
        }
        Expression::IndexAccess { target, index } => {
            let t = lower_expression(ctx, target);
            let i = lower_expression(ctx, index);
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::IndexAccess {
                dest,
                target: t,
                index: i,
            });
            dest
        }
        Expression::StructLit(type_name, fields) => {
            let mut field_regs = HashMap::new();
            for (name, expr) in fields {
                field_regs.insert(name.clone(), lower_expression(ctx, expr));
            }
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::StructLit {
                dest,
                fields: field_regs,
                type_name: type_name.clone(),
            });
            dest
        }
        Expression::TopologyLit(fields) => {
            let mut field_regs = HashMap::new();
            for (name, expr) in fields {
                field_regs.insert(name.clone(), lower_expression(ctx, expr));
            }
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::TopologyLit {
                dest,
                fields: field_regs,
            });
            dest
        }
        Expression::ArrayLiteral(elements) => {
            let mut elem_regs = Vec::new();
            for e in elements {
                elem_regs.push(lower_expression(ctx, e));
            }
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::ArrayLit {
                dest,
                elements: elem_regs,
            });
            dest
        }
        Expression::ChannelReceive(chan_id) => {
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::ChanRecv {
                dest,
                chan_id: chan_id.clone(),
            });
            dest
        }
        Expression::Deferred {
            capability,
            params,
            deadline_ms,
        } => {
            let dest = ctx.alloc_reg();
            ctx.push(Instruction::Defer {
                dest,
                cap: ictl_core::Capability {
                    path: capability.clone(),
                    parameters: params.clone(),
                },
                deadline_ms: *deadline_ms,
            });
            dest
        }
    }
}
