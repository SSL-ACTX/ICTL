use crate::analysis::analyzer::{
    EntropicAnalyzer, SemanticError, SemanticErrorKind,
};
use crate::frontend::ast::*;

pub(crate) fn analyze_expression(
    analyzer: &mut EntropicAnalyzer,
    expr: &Expression,
) -> Result<(), SemanticError> {
    match expr {
        Expression::Null => Ok(()),
        Expression::Call { routine, args } => {
            let (params, _taking_ms) = analyzer
                .routines
                .get(routine)
                .ok_or_else(|| {
                    analyzer.annotate(SemanticErrorKind::EntropyMismatch(format!(
                        "unknown routine {}",
                        routine
                    )))
                })?
                .clone();

            if args.len() != params.len() {
                return Err(analyzer.annotate(SemanticErrorKind::EntropyMismatch(
                    format!(
                        "routine {} expects {} args, got {}",
                        routine,
                        params.len(),
                        args.len()
                    ),
                )));
            }

            for (arg_expr, (mode, _param_name)) in args.iter().zip(params.iter()) {
                analyze_expression_nonconsuming(analyzer, arg_expr)?;

                match mode {
                    ParamMode::Consume => {
                        if let Expression::Identifier(name) = arg_expr {
                            analyzer.mark_consumed(name)?;
                        } else {
                            return Err(analyzer.annotate(
                                SemanticErrorKind::EntropyMismatch(
                                    "consume param must be identifier".into(),
                                ),
                            ));
                        }
                    }
                    ParamMode::Clone => {
                        if let Expression::Identifier(name) = arg_expr {
                            let state = analyzer
                                .branch_contexts
                                .get(&analyzer.current_branch)
                                .unwrap();
                            if state.consumed.contains(name) {
                                return Err(analyzer.annotate(
                                    SemanticErrorKind::UseAfterConsume(name.clone()),
                                ));
                            }
                        }
                    }
                    ParamMode::Decay => {
                        if let Expression::Identifier(name) = arg_expr {
                            analyzer.mark_consumed(name)?;
                        }
                    }
                    ParamMode::Peek => {}
                }
            }
            Ok(())
        }
        Expression::Identifier(name) => analyzer.mark_consumed(name),
        Expression::FieldAccess { target, .. } => {
            if let Expression::Identifier(name) = &**target {
                if analyzer.inspection_depth == 0 {
                    analyzer.mark_decayed(name)?;
                }
                Ok(())
            } else {
                analyze_expression(analyzer, target)
            }
        }
        Expression::CloneOp(name) => {
            let state = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .unwrap();
            if state.consumed.contains(name) {
                return Err(analyzer
                    .annotate(SemanticErrorKind::UseAfterConsume(name.clone())));
            }
            Ok(())
        }
        Expression::StructLit(fields) | Expression::TopologyLit(fields) => {
            for (_, inner_expr) in fields {
                analyze_expression(analyzer, inner_expr)?;
            }
            Ok(())
        }
        Expression::IndexAccess { target, index } => {
            if let Expression::Identifier(name) = &**target {
                analyzer.mark_decayed(name)?;
            } else {
                analyze_expression(analyzer, target)?;
            }
            analyze_expression_nonconsuming(analyzer, index)?;
            Ok(())
        }
        Expression::ChannelReceive(_)
        | Expression::Literal(_)
        | Expression::Integer(_)
        | Expression::ArrayLiteral(_)
        | Expression::Deferred { .. } => Ok(()),
        Expression::BinaryOp { left, right, .. } => {
            analyze_expression(analyzer, left)?;
            analyze_expression(analyzer, right)?;
            Ok(())
        }
    }
}

pub(crate) fn analyze_expression_nonconsuming(
    analyzer: &mut EntropicAnalyzer,
    expr: &Expression,
) -> Result<(), SemanticError> {
    match expr {
        Expression::Call { .. } => analyze_expression(analyzer, expr),
        Expression::Identifier(_) => Ok(()),
        Expression::FieldAccess { target, .. } => {
            analyze_expression_nonconsuming(analyzer, target)
        }
        Expression::CloneOp(name) => {
            let state = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .unwrap();
            if state.consumed.contains(name) {
                return Err(analyzer
                    .annotate(SemanticErrorKind::UseAfterConsume(name.clone())));
            }
            Ok(())
        }
        Expression::StructLit(fields) | Expression::TopologyLit(fields) => {
            for (_, inner_expr) in fields {
                analyze_expression_nonconsuming(analyzer, inner_expr)?;
            }
            Ok(())
        }
        Expression::IndexAccess { target, index } => {
            analyze_expression_nonconsuming(analyzer, target)?;
            analyze_expression_nonconsuming(analyzer, index)?;
            Ok(())
        }

        Expression::ArrayLiteral(elements) => {
            for inner_expr in elements {
                analyze_expression_nonconsuming(analyzer, inner_expr)?;
            }
            Ok(())
        }
        Expression::ChannelReceive(_)
        | Expression::Literal(_)
        | Expression::Integer(_)
        | Expression::Null
        | Expression::Deferred { .. } => Ok(()),
        Expression::BinaryOp { left, right, .. } => {
            analyze_expression_nonconsuming(analyzer, left)?;
            analyze_expression_nonconsuming(analyzer, right)?;
            Ok(())
        }
    }
}

pub(crate) fn estimate_expression_cost(
    analyzer: &EntropicAnalyzer,
    expr: &Expression,
) -> u64 {
    match expr {
        Expression::Call { routine, args } => {
            let arg_cost: u64 = args
                .iter()
                .map(|a| estimate_expression_cost(analyzer, a))
                .sum();
            let taking_ms =
                analyzer.routines.get(routine).map(|(_, t)| *t).unwrap_or(0);
            arg_cost + taking_ms
        }
        Expression::BinaryOp { left, right, .. } => {
            1 + estimate_expression_cost(analyzer, left)
                + estimate_expression_cost(analyzer, right)
        }
        Expression::StructLit(fields) | Expression::TopologyLit(fields) => {
            1 + fields
                .values()
                .map(|v| estimate_expression_cost(analyzer, v))
                .sum::<u64>()
        }
        Expression::IndexAccess { target, index } => {
            1 + estimate_expression_cost(analyzer, target)
                + estimate_expression_cost(analyzer, index)
        }
        Expression::ArrayLiteral(elements) => {
            1 + elements
                .iter()
                .map(|e| estimate_expression_cost(analyzer, e))
                .sum::<u64>()
        }
        Expression::FieldAccess { .. }
        | Expression::CloneOp(_)
        | Expression::ChannelReceive(_)
        | Expression::Identifier(_)
        | Expression::Literal(_)
        | Expression::Integer(_)
        | Expression::Null
        | Expression::Deferred { .. } => 1,
    }
}
