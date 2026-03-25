use crate::frontend::ast::{BinaryOperator, Expression, ParamMode};
use crate::runtime::memory::{EntropicState, MemoryError, Payload};
use crate::runtime::vm::error::TemporalError;
use crate::runtime::vm::state::{Timeline, Vm};
use std::collections::HashMap;

impl Vm {
    fn evaluate_binary_operation(
        &self,
        left_value: Payload,
        right_value: Payload,
        op: &BinaryOperator,
    ) -> Result<Payload, TemporalError> {
        let l = match left_value {
            Payload::Integer(i) => i,
            Payload::String(ref s) => s.parse::<i64>().unwrap_or(0),
            _ => 0,
        };
        let r = match right_value {
            Payload::Integer(i) => i,
            Payload::String(ref s) => s.parse::<i64>().unwrap_or(0),
            _ => 0,
        };

        let result = match op {
            BinaryOperator::Add => l + r,
            BinaryOperator::Sub => l - r,
            BinaryOperator::Mul => l * r,
            BinaryOperator::Div => {
                if r == 0 {
                    return Err(TemporalError::EvalError("Division by zero".into()));
                }
                l / r
            }
            BinaryOperator::Eq => (l == r) as i64,
            BinaryOperator::Neq => (l != r) as i64,
            BinaryOperator::Lt => (l < r) as i64,
            BinaryOperator::Gt => (l > r) as i64,
            BinaryOperator::Le => (l <= r) as i64,
            BinaryOperator::Ge => (l >= r) as i64,
        };

        Ok(Payload::Integer(result))
    }

    fn evaluate_struct_literal(
        &mut self,
        branch_id: &str,
        fields: &HashMap<String, Expression>,
        consuming: bool,
    ) -> Result<Payload, TemporalError> {
        let mut evaluated_fields = HashMap::new();
        for (name, inner_expr) in fields {
            let payload = self
                .evaluate_expression_with_usage(branch_id, inner_expr, consuming)?;
            evaluated_fields.insert(name.clone(), EntropicState::Valid(payload));
        }
        Ok(Payload::Struct(evaluated_fields))
    }

    fn evaluate_topology_literal(
        &mut self,
        branch_id: &str,
        fields: &HashMap<String, Expression>,
        consuming: bool,
    ) -> Result<Payload, TemporalError> {
        let mut evaluated_fields = HashMap::new();
        for (name, inner_expr) in fields {
            let payload = self
                .evaluate_expression_with_usage(branch_id, inner_expr, consuming)?;
            evaluated_fields.insert(name.clone(), EntropicState::Valid(payload));
        }
        Ok(Payload::Topology(evaluated_fields))
    }

    fn evaluate_array_literal(
        &mut self,
        branch_id: &str,
        elements: &[Expression],
        consuming: bool,
    ) -> Result<Payload, TemporalError> {
        let mut values = Vec::new();
        for expr in elements {
            values.push(
                self.evaluate_expression_with_usage(branch_id, expr, consuming)?,
            );
        }
        Ok(Payload::Array(values))
    }

    fn evaluate_expression_with_usage(
        &mut self,
        branch_id: &str,
        expr: &Expression,
        consuming: bool,
    ) -> Result<Payload, TemporalError> {
        match expr {
            Expression::Literal(val) => Ok(Payload::String(val.clone())),
            Expression::Integer(v) => Ok(Payload::Integer(*v)),
            Expression::Identifier(name) => {
                let (payload, is_entangled) = {
                    let branch = self.get_branch_mut(branch_id)?;
                    match branch.arena.bindings.get(name) {
                        Some(EntropicState::Pending(_)) => {
                            return Err(TemporalError::EvalError(format!(
                                "pending value {} must be awaited",
                                name
                            )))
                        }
                        _ => {
                            if consuming {
                                let val = branch.arena.consume(name)?;
                                (val, true)
                            } else {
                                let payload =
                                    branch.arena.peek(name).ok_or_else(|| {
                                        TemporalError::MemoryFault(
                                            MemoryError::AlreadyConsumed,
                                        )
                                    })?;
                                (payload, false)
                            }
                        }
                    }
                };
                if is_entangled && consuming {
                    self.propagate_entanglement(branch_id, name)?;
                }
                Ok(payload)
            }
            Expression::FieldAccess { parent, field } => {
                let (payload, is_consuming) = {
                    let branch = self.get_branch_mut(branch_id)?;
                    if let Some(EntropicState::Pending(_)) =
                        branch.arena.bindings.get(parent)
                    {
                        return Err(TemporalError::EvalError(format!(
                            "pending value {} must be awaited",
                            parent
                        )));
                    }

                    if consuming {
                        let val = branch.arena.consume_field(parent, field)?;
                        (val, true)
                    } else {
                        let parent_payload =
                            branch.arena.peek(parent).ok_or_else(|| {
                                TemporalError::MemoryFault(
                                    MemoryError::AlreadyConsumed,
                                )
                            })?;
                        match parent_payload {
                            Payload::Struct(ref fields)
                            | Payload::Topology(ref fields) => {
                                match fields.get(field) {
                                    Some(EntropicState::Valid(payload)) => {
                                        (payload.clone(), false)
                                    }
                                    _ => {
                                        return Err(TemporalError::MemoryFault(
                                            MemoryError::AlreadyConsumed,
                                        ))
                                    }
                                }
                            }
                            _ => {
                                return Err(TemporalError::MemoryFault(
                                    MemoryError::NotAStruct,
                                ))
                            }
                        }
                    }
                };
                if is_consuming {
                    self.propagate_field_decay(branch_id, parent, field)?;
                }
                Ok(payload)
            }
            Expression::IndexAccess { target, index } => {
                let target_payload =
                    self.evaluate_expression_nonconsuming(branch_id, target)?;
                let index_payload =
                    self.evaluate_expression_nonconsuming(branch_id, index)?;

                let index_str = match index_payload {
                    Payload::String(s) => s,
                    Payload::Integer(i) => i.to_string(),
                    _ => {
                        return Err(TemporalError::EvalError(
                            "Index must be string or integer".into(),
                        ))
                    }
                };

                match **target {
                    Expression::Identifier(ref name) => {
                        let (payload, is_consuming) = {
                            let branch = self.get_branch_mut(branch_id)?;
                            if consuming {
                                let val =
                                    branch.arena.consume_field(name, &index_str)?;
                                (val, true)
                            } else {
                                match target_payload {
                                    Payload::Topology(ref fields)
                                    | Payload::Struct(ref fields) => {
                                        match fields.get(&index_str) {
                                            Some(EntropicState::Valid(p)) => {
                                                (p.clone(), false)
                                            }
                                            _ => {
                                                return Err(
                                                    TemporalError::MemoryFault(
                                                        MemoryError::AlreadyConsumed,
                                                    ),
                                                )
                                            }
                                        }
                                    }
                                    _ => {
                                        return Err(TemporalError::MemoryFault(
                                            MemoryError::NotAStruct,
                                        ))
                                    }
                                }
                            }
                        };
                        if is_consuming {
                            self.propagate_field_decay(branch_id, name, &index_str)?;
                        }
                        Ok(payload)
                    }
                    _ => {
                        // For non-identifier targets, we just peek into the evaluated value
                        match target_payload {
                            Payload::Topology(ref fields)
                            | Payload::Struct(ref fields) => {
                                match fields.get(&index_str) {
                                    Some(EntropicState::Valid(p)) => Ok(p.clone()),
                                    _ => Err(TemporalError::MemoryFault(
                                        MemoryError::AlreadyConsumed,
                                    )),
                                }
                            }
                            _ => Err(TemporalError::MemoryFault(
                                MemoryError::NotAStruct,
                            )),
                        }
                    }
                }
            }
            Expression::CloneOp(name) => {
                let branch = self.get_branch_mut(branch_id)?;
                let payload = branch.arena.peek(name).ok_or_else(|| {
                    TemporalError::MemoryFault(MemoryError::AlreadyConsumed)
                })?;
                let cost = branch.arena.calculate_clone_cost(&payload, 1);
                branch.consume_budget(cost)?;
                Ok(payload)
            }
            Expression::StructLit(fields) => {
                self.evaluate_struct_literal(branch_id, fields, consuming)
            }
            Expression::TopologyLit(fields) => {
                self.evaluate_topology_literal(branch_id, fields, consuming)
            }
            Expression::ArrayLiteral(elements) => {
                self.evaluate_array_literal(branch_id, elements, consuming)
            }
            Expression::ChannelReceive(chan_id) => {
                let chan = self.channels.get_mut(chan_id).ok_or_else(|| {
                    TemporalError::ChannelFault(format!(
                        "Channel not found: {}",
                        chan_id
                    ))
                })?;
                let payload = chan.pop_front().ok_or_else(|| {
                    TemporalError::ChannelFault(format!(
                        "Channel empty: {}",
                        chan_id
                    ))
                })?;
                Ok(payload)
            }
            Expression::Deferred { .. } => {
                Ok(Payload::String("pending".to_string()))
            }
            Expression::Call { routine, args } => {
                self.evaluate_call(branch_id, routine, args)
            }
            Expression::BinaryOp { left, op, right } => {
                let left_val =
                    self.evaluate_expression_with_usage(branch_id, left, consuming)?;
                let right_val = self
                    .evaluate_expression_with_usage(branch_id, right, consuming)?;
                self.evaluate_binary_operation(left_val, right_val, op)
            }
        }
    }

    pub fn evaluate_expression_nonconsuming(
        &mut self,
        branch_id: &str,
        expr: &Expression,
    ) -> Result<Payload, TemporalError> {
        self.evaluate_expression_with_usage(branch_id, expr, false)
    }

    pub fn evaluate_expression(
        &mut self,
        branch_id: &str,
        expr: &Expression,
    ) -> Result<Payload, TemporalError> {
        self.evaluate_expression_with_usage(branch_id, expr, true)
    }

    fn evaluate_call(
        &mut self,
        branch_id: &str,
        routine: &str,
        args: &[Expression],
    ) -> Result<Payload, TemporalError> {
        let routine_def = self
            .routines
            .get(routine)
            .ok_or_else(|| {
                TemporalError::EvalError(format!("unknown routine {}", routine))
            })?
            .clone();
        let params = routine_def.params.clone();
        let taking_ms = routine_def.taking_ms.unwrap_or(0);

        if args.len() != params.len() {
            return Err(TemporalError::EvalError(format!(
                "routine call expects {} args, got {}",
                params.len(),
                args.len()
            )));
        }

        let (mut param_values, caller_capacity, caller_entropy_mode) = {
            let caller_branch_inner = self.get_branch_mut(branch_id)?;

            let mut values: Vec<Option<Payload>> = Vec::new();

            for (arg_expr, (mode, _param_name)) in args.iter().zip(params.iter()) {
                if let Expression::Identifier(var) = arg_expr {
                    let result = match mode {
                        ParamMode::Consume => {
                            let v = caller_branch_inner.arena.consume(var)?;
                            Some(v)
                        }
                        ParamMode::Clone => {
                            let v = caller_branch_inner
                                .arena
                                .peek(var)
                                .ok_or(MemoryError::AlreadyConsumed)?;
                            Some(v)
                        }
                        ParamMode::Decay => {
                            let v = caller_branch_inner
                                .arena
                                .peek(var)
                                .ok_or(MemoryError::AlreadyConsumed)?;
                            caller_branch_inner.arena.decay(var)?;
                            Some(v)
                        }
                        ParamMode::Peek => {
                            let v = caller_branch_inner
                                .arena
                                .peek(var)
                                .ok_or(MemoryError::AlreadyConsumed)?;
                            Some(v)
                        }
                    };
                    values.push(result);
                } else {
                    values.push(None);
                }
            }

            (
                values,
                caller_branch_inner.arena.capacity,
                caller_branch_inner.entropy_mode,
            )
        };

        for (i, (arg_expr, _)) in args.iter().zip(params.iter()).enumerate() {
            if param_values[i].is_none() {
                let v =
                    self.evaluate_expression_with_usage(branch_id, arg_expr, true)?;
                param_values[i] = Some(v);
            }
        }

        let param_values: Vec<Payload> = param_values
            .into_iter()
            .map(|opt| opt.expect("param value must exist"))
            .collect();

        let child_id = format!("__routine_{}_{}", taking_ms, self.global_clock);
        let mut child =
            Timeline::new(child_id.clone(), caller_capacity, self.global_clock);
        child.entropy_mode = caller_entropy_mode;

        for ((_, param_name), val) in params.iter().zip(param_values) {
            child
                .arena
                .insert(param_name.clone(), EntropicState::Valid(val))?;
        }

        self.active_branches.insert(child_id.clone(), child);

        for stmt in &routine_def.body {
            self.execute_statement(&child_id, stmt)?;
        }

        let child_branch = self
            .active_branches
            .remove(&child_id)
            .ok_or_else(|| TemporalError::BranchNotFound(child_id.clone()))?;

        if child_branch.local_clock > taking_ms {
            return Err(TemporalError::WatchdogBite(child_id.clone(), taking_ms));
        }

        let call_charge = taking_ms.saturating_sub(1);
        let caller_branch = self.get_branch_mut(branch_id)?;
        if call_charge > 0 {
            caller_branch.local_clock += call_charge;
            caller_branch.consume_budget(call_charge)?;
        }

        let result = match child_branch.arena.peek("yielded") {
            Some(Payload::Array(mut arr)) => {
                if !arr.is_empty() {
                    arr.remove(0)
                } else {
                    Payload::String("void".to_string())
                }
            }
            _ => Payload::String("void".to_string()),
        };

        Ok(result)
    }

    pub fn evaluate_entropic_state(
        &mut self,
        branch_id: &str,
        expr: &Expression,
    ) -> Result<EntropicState, TemporalError> {
        match expr {
            Expression::Identifier(name) => {
                let state = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.consume_entropic(name)?
                };
                self.propagate_entanglement(branch_id, name)?;
                Ok(state)
            }
            Expression::IndexAccess { target, index } => {
                let index_payload =
                    self.evaluate_expression_nonconsuming(branch_id, index)?;
                let index_str = match index_payload {
                    Payload::String(s) => s,
                    Payload::Integer(i) => i.to_string(),
                    _ => {
                        return Err(TemporalError::EvalError(
                            "Index must be string or integer".into(),
                        ))
                    }
                };
                match **target {
                    Expression::Identifier(ref name) => {
                        let state = {
                            let branch = self.get_branch_mut(branch_id)?;
                            branch.arena.consume_field_entropic(name, &index_str)?
                        };
                        self.propagate_field_decay(branch_id, name, &index_str)?;
                        Ok(state)
                    }
                    _ => {
                        let target_payload = self
                            .evaluate_expression_nonconsuming(branch_id, target)?;
                        match target_payload {
                            Payload::Struct(fields) | Payload::Topology(fields) => {
                                fields.get(&index_str).cloned().ok_or(
                                    TemporalError::MemoryFault(
                                        MemoryError::KeyNotFound(index_str),
                                    ),
                                )
                            }
                            _ => Err(TemporalError::MemoryFault(
                                MemoryError::NotAStruct,
                            )),
                        }
                    }
                }
            }
            Expression::FieldAccess { parent, field } => {
                let state = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.consume_field_entropic(parent, field)?
                };
                self.propagate_field_decay(branch_id, parent, field)?;
                Ok(state)
            }
            _ => {
                let payload = self.evaluate_expression(branch_id, expr)?;
                Ok(EntropicState::Valid(payload))
            }
        }
    }
}
