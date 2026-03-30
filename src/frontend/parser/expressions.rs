use crate::frontend::ast::*;
use crate::frontend::parser::Rule;
use std::collections::HashMap;

pub(crate) fn parse_expression(pair: pest::iterators::Pair<Rule>) -> Expression {
    match pair.as_rule() {
        Rule::expression
        | Rule::relational_expr
        | Rule::additive_expr
        | Rule::multiplicative_expr => {
            let mut inner = pair.into_inner();
            let first = inner.next().map(parse_expression);
            if first.is_none() {
                return Expression::Literal("void".into());
            }
            let mut left = first.unwrap();
            while let Some(op_pair) = inner.next() {
                let op = match op_pair.as_str() {
                    "+" => crate::frontend::ast::BinaryOperator::Add,
                    "-" => crate::frontend::ast::BinaryOperator::Sub,
                    "*" => crate::frontend::ast::BinaryOperator::Mul,
                    "/" => crate::frontend::ast::BinaryOperator::Div,
                    "==" => crate::frontend::ast::BinaryOperator::Eq,
                    "!=" => crate::frontend::ast::BinaryOperator::Neq,
                    "<" => crate::frontend::ast::BinaryOperator::Lt,
                    ">" => crate::frontend::ast::BinaryOperator::Gt,
                    "<=" => crate::frontend::ast::BinaryOperator::Le,
                    ">=" => crate::frontend::ast::BinaryOperator::Ge,
                    _ => crate::frontend::ast::BinaryOperator::Eq,
                };
                if let Some(right) = inner.next() {
                    let right_expr = parse_expression(right);
                    left = Expression::BinaryOp {
                        left: Box::new(left),
                        op,
                        right: Box::new(right_expr),
                    };
                }
            }
            left
        }
        Rule::unary_expr => {
            let mut inner = pair.into_inner();
            if let Some(first) = inner.next() {
                if first.as_str() == "-" {
                    let expr = parse_expression(inner.next().unwrap());
                    let zero = Expression::Integer(0);
                    return Expression::BinaryOp {
                        left: Box::new(zero),
                        op: crate::frontend::ast::BinaryOperator::Sub,
                        right: Box::new(expr),
                    };
                }
                parse_expression(first)
            } else {
                Expression::Literal("void".into())
            }
        }
        Rule::primary_expr => {
            let mut inner = pair.into_inner();
            let mut expr = parse_expression(inner.next().unwrap());
            while let Some(access_pair) = inner.next() {
                match access_pair.as_rule() {
                    Rule::index_access => {
                        let index = parse_expression(
                            access_pair.into_inner().next().unwrap(),
                        );
                        expr = Expression::IndexAccess {
                            target: Box::new(expr),
                            index: Box::new(index),
                        };
                    }
                    Rule::field_access_tail => {
                        let field = access_pair
                            .into_inner()
                            .next()
                            .map(|p| p.as_str().to_string())
                            .unwrap_or_default();
                        expr = Expression::FieldAccess {
                            target: Box::new(expr),
                            field,
                        };
                    }
                    _ => {}
                }
            }
            expr
        }
        Rule::base_expr => parse_expression(pair.into_inner().next().unwrap()),
        Rule::defer_expr => {
            let mut inner = pair.into_inner();
            let capability = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mut params = std::collections::HashMap::new();
            let mut deadline_ms = 0;

            if let Some(param_list) = inner.next() {
                for param in param_list.into_inner() {
                    let mut param_inner = param.into_inner();
                    if let (Some(key), Some(value)) =
                        (param_inner.next(), param_inner.next())
                    {
                        params.insert(
                            key.as_str().replace("\"", ""),
                            value.as_str().replace("\"", ""),
                        );
                    }
                }
            }

            if let Some(amount) = inner.next() {
                deadline_ms = amount.as_str().parse::<u64>().unwrap_or(0);
            }

            Expression::Deferred {
                capability,
                params,
                deadline_ms,
            }
        }
        Rule::call_expr => {
            let mut inner = pair.into_inner();
            let routine = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mut args = Vec::new();
            if let Some(expr_list) = inner.next() {
                for e in expr_list.into_inner() {
                    args.push(parse_expression(e));
                }
            }
            Expression::Call { routine, args }
        }
        Rule::integer_literal => {
            Expression::Integer(pair.as_str().parse::<i64>().unwrap_or(0))
        }
        Rule::bool_literal => Expression::Boolean(pair.as_str() == "true"),
        Rule::string_literal => Expression::Literal(pair.as_str().replace("\"", "")),
        Rule::identifier_expr | Rule::identifier => {
            Expression::Identifier(pair.as_str().to_string())
        }
        Rule::clone_op => Expression::CloneOp(
            pair.into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default(),
        ),
        Rule::chan_recv_expr => Expression::ChannelReceive(
            pair.into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default(),
        ),
        Rule::struct_lit | Rule::topology_lit => {
            let rule = pair.as_rule();
            let mut fields = HashMap::new();
            if let Some(params) = pair.into_inner().next() {
                for p in params.into_inner() {
                    let mut p_inner = p.into_inner();
                    if let (Some(k), Some(v)) = (p_inner.next(), p_inner.next()) {
                        fields.insert(
                            k.as_str().replace("\"", ""),
                            parse_expression(v),
                        );
                    }
                }
            }
            if rule == Rule::struct_lit {
                Expression::StructLit(fields)
            } else {
                Expression::TopologyLit(fields)
            }
        }
        Rule::array_lit => {
            let mut elements = Vec::new();
            for expr_pair in pair.into_inner() {
                elements.push(parse_expression(expr_pair));
            }
            Expression::ArrayLiteral(elements)
        }
        Rule::field_access => {
            let mut inner = pair.into_inner();
            let parent = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let field = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            Expression::FieldAccess {
                target: Box::new(Expression::Identifier(parent)),
                field,
            }
        }
        Rule::null => Expression::Null,
        _ => Expression::Literal(pair.as_str().to_string()),
    }
}
