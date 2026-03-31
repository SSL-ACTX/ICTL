use crate::analysis::analyzer::{
    EntropicAnalyzer, SemanticError, SemanticErrorKind,
};
use crate::analysis::types::Type;
use crate::frontend::ast::*;

pub(crate) fn infer_expression_type(
    analyzer: &EntropicAnalyzer,
    expr: &Expression,
) -> Result<Type, SemanticError> {
    match expr {
        Expression::Null => Ok(Type::Unknown),
        Expression::Boolean(_) => Ok(Type::Bool),
        Expression::Integer(_) => Ok(Type::Integer),
        Expression::Literal(_) => Ok(Type::String),
        Expression::Identifier(name) => match analyzer.get_variable_type(name) {
            Some(typ) => Ok(typ),
            None => Err(analyzer
                .annotate(SemanticErrorKind::UndefinedVariable(name.to_string()))),
        },
        Expression::StructLit(fields) => {
            let mut schema = std::collections::HashMap::new();
            for (k, v) in fields {
                schema.insert(k.clone(), infer_expression_type(analyzer, v)?);
            }
            Ok(Type::Struct(schema))
        }
        Expression::TopologyLit(fields) => {
            let mut schema = std::collections::HashMap::new();
            for (k, v) in fields {
                schema.insert(k.clone(), infer_expression_type(analyzer, v)?);
            }
            Ok(Type::Topology(schema))
        }
        Expression::ArrayLiteral(elements) => {
            let elem_types: Vec<Type> = elements
                .iter()
                .map(|e| infer_expression_type(analyzer, e))
                .collect::<Result<_, _>>()?;
            if elem_types.is_empty() {
                Ok(Type::Array(Box::new(Type::Unknown)))
            } else {
                let first = elem_types[0].clone();
                if elem_types.iter().all(|t| t == &first) {
                    Ok(Type::Array(Box::new(first)))
                } else {
                    Ok(Type::Array(Box::new(Type::Unknown)))
                }
            }
        }
        Expression::ChannelReceive(_) => Ok(Type::Unknown),
        Expression::Deferred { .. } => Ok(Type::Unknown),
        Expression::Call { routine, .. } => {
            if let Some(info) = analyzer.routines.get(routine) {
                Ok(info.return_type.clone())
            } else {
                Ok(Type::Unknown)
            }
        },
        Expression::FieldAccess { target, field } => {
            let t = infer_expression_type(analyzer, target)?;
            let resolved_t = analyzer.resolve_type(&t);
            match resolved_t {
                Type::Unknown => Ok(Type::Unknown),
                Type::Struct(fields) | Type::Topology(fields) => {
                    fields.get(field).cloned().ok_or_else(|| {
                        analyzer.annotate(SemanticErrorKind::TypeMismatch(format!(
                            "field '{}' not found",
                            field
                        )))
                    })
                }
                _ => Err(analyzer.annotate(SemanticErrorKind::TypeMismatch(
                    "field access on non-struct/topology".into(),
                ))),
            }
        }
        Expression::IndexAccess { .. } => Ok(Type::Unknown),
        Expression::CloneOp(_) => Ok(Type::Unknown),
        Expression::BinaryOp { left, op, right } => {
            let left_type = infer_expression_type(analyzer, left)?;
            let right_type = infer_expression_type(analyzer, right)?;
            match op {
                crate::frontend::ast::BinaryOperator::Add
                | crate::frontend::ast::BinaryOperator::Sub
                | crate::frontend::ast::BinaryOperator::Mul
                | crate::frontend::ast::BinaryOperator::Div => {
                    if left_type == Type::Integer && right_type == Type::Integer {
                        Ok(Type::Integer)
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
                        Ok(Type::Bool)
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
                    if left_type == Type::Integer && right_type == Type::Integer {
                        Ok(Type::Bool)
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
            let info = analyzer
                .routines
                .get(routine)
                .cloned()
                .ok_or_else(|| {
                    analyzer.annotate(SemanticErrorKind::EntropyMismatch(format!(
                        "unknown routine {}",
                        routine
                    )))
                })?;

            if args.len() != info.params.len() {
                return Err(analyzer.annotate(SemanticErrorKind::EntropyMismatch(
                    format!(
                        "routine {} expects {} args, got {}",
                        routine,
                        info.params.len(),
                        args.len()
                    ),
                )));
            }

            for (arg_expr, (mode, _param_name, expected_type)) in
                args.iter().zip(info.params.iter())
            {
                let arg_type = infer_expression_type(analyzer, arg_expr)?;

                if !analyzer.types_compatible(&expected_type, &arg_type) {
                    return Err(analyzer.annotate(SemanticErrorKind::TypeMismatch(
                        format!(
                            "routine {} arg type mismatch: expected {:?}, got {:?}",
                            routine,
                            expected_type,
                            arg_type
                        ),
                    )));
                }

                analyze_expression_nonconsuming(analyzer, arg_expr)?;

                match mode {
                    ParamMode::Consume => {
                        if let Expression::Identifier(name) = arg_expr {
                            analyzer.mark_consumed(name)?;
                        }
                        // non-identifiers are treated as value literals and do not consume existing variables
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
            let taking_ms = analyzer
                .routines
                .get(routine)
                .map(|info| info.taking_ms)
                .unwrap_or(0);
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
