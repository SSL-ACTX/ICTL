use crate::analysis::analyzer::{
    EntropicAnalyzer, SemanticError, SemanticErrorKind, ValueType,
};
use crate::frontend::ast::*;

pub(crate) fn infer_expression_type(
    analyzer: &EntropicAnalyzer,
    expr: &Expression,
) -> Result<ValueType, SemanticError> {
    match expr {
        Expression::Null => Ok(ValueType::Null),
        Expression::Boolean(_) => Ok(ValueType::Bool),
        Expression::Integer(_) => Ok(ValueType::Integer),
        Expression::Literal(_) => Ok(ValueType::String),
        Expression::Identifier(name) => match analyzer.get_variable_type(name) {
            Some(typ) => Ok(typ),
            None => Err(analyzer
                .annotate(SemanticErrorKind::UndefinedVariable(name.to_string()))),
        },
        Expression::StructLit(_) => Ok(ValueType::Struct),
        Expression::TopologyLit(_) => Ok(ValueType::Topology),
        Expression::ArrayLiteral(_) => Ok(ValueType::Array),
        Expression::ChannelReceive(_) => Ok(ValueType::Unknown),
        Expression::Deferred { .. } => Ok(ValueType::Unknown),
        Expression::Call { .. } => Ok(ValueType::Unknown),
        Expression::FieldAccess { .. } => Ok(ValueType::Unknown),
        Expression::IndexAccess { .. } => Ok(ValueType::Unknown),
        Expression::CloneOp(_) => Ok(ValueType::Unknown),
        Expression::BinaryOp { left, op, right } => {
            let left_type = infer_expression_type(analyzer, left)?;
            let right_type = infer_expression_type(analyzer, right)?;
            match op {
                crate::frontend::ast::BinaryOperator::Add
                | crate::frontend::ast::BinaryOperator::Sub
                | crate::frontend::ast::BinaryOperator::Mul
                | crate::frontend::ast::BinaryOperator::Div => {
                    if left_type == ValueType::Integer
                        && right_type == ValueType::Integer
                    {
                        Ok(ValueType::Integer)
                    } else {
                        Err(analyzer.annotate(SemanticErrorKind::TypeMismatch(
                            format!(
                                "cannot apply '{:?}' to {:?} and {:?}",
                                op, left_type, right_type
                            ),
                        )))
                    }
                }
                crate::frontend::ast::BinaryOperator::Eq
                | crate::frontend::ast::BinaryOperator::Neq => {
                    if left_type == right_type {
                        Ok(ValueType::Bool)
                    } else {
                        Err(analyzer.annotate(SemanticErrorKind::TypeMismatch(
                            format!(
                                "cannot compare {:?} with {:?}",
                                left_type, right_type
                            ),
                        )))
                    }
                }
                crate::frontend::ast::BinaryOperator::Lt
                | crate::frontend::ast::BinaryOperator::Gt
                | crate::frontend::ast::BinaryOperator::Le
                | crate::frontend::ast::BinaryOperator::Ge => {
                    if left_type == ValueType::Integer
                        && right_type == ValueType::Integer
                    {
                        Ok(ValueType::Bool)
                    } else {
                        Err(analyzer.annotate(SemanticErrorKind::TypeMismatch(
                            format!(
                                "cannot order compare {:?} and {:?}",
                                left_type, right_type
                            ),
                        )))
                    }
                }
            }
        }
    }
}

pub(crate) fn analyze_expression(
    analyzer: &mut EntropicAnalyzer,
    expr: &Expression,
) -> Result<(), SemanticError> {
    infer_expression_type(analyzer, expr)?;
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
        | Expression::Boolean(_)
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
    infer_expression_type(analyzer, expr)?;
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
        | Expression::Boolean(_)
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
        | Expression::Boolean(_)
        | Expression::Null
        | Expression::Deferred { .. } => 1,
    }
}
