use crate::frontend::ast::Statement;
use crate::runtime::vm::error::TemporalError;
use crate::runtime::vm::state::Vm;

impl Vm {
    pub fn estimate_block_cost(
        &self,
        block: &[crate::frontend::ast::SpannedStatement],
    ) -> u64 {
        block
            .iter()
            .map(|stmt| self.estimate_statement_cost(&stmt.stmt))
            .sum()
    }

    pub fn estimate_statement_cost(&self, stmt: &Statement) -> u64 {
        let base = 1;
        let extra = match stmt {
            Statement::NetworkRequest { .. } => 5,
            Statement::Split { .. }
            | Statement::Merge { .. }
            | Statement::Anchor(_)
            | Statement::Rewind(_)
            | Statement::Commit(_)
            | Statement::Send { .. }
            | Statement::ChannelOpen { .. }
            | Statement::ChannelSend { .. }
            | Statement::AcausalReset { .. }
            | Statement::Capability(_)
            | Statement::Assignment { .. }
            | Statement::Expression(_)
            | Statement::Print(_) => 0,
            Statement::RelativisticBlock { body, .. } => {
                self.estimate_block_cost(body)
            }
            Statement::Isolate(block) => self.estimate_block_cost(&block.body),
            Statement::Watchdog { recovery, .. } => {
                self.estimate_block_cost(recovery)
            }
            Statement::Debug(_) => 1,
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                1 + self.estimate_block_cost(then_branch).max(
                    self.estimate_block_cost(
                        else_branch.as_ref().unwrap_or(&Vec::new()),
                    ),
                )
            }
            Statement::For { pacing_ms, .. } => pacing_ms.unwrap_or(1),
            Statement::Speculate { body, fallback, .. } => {
                let fallback_cost = self
                    .estimate_block_cost(fallback.as_ref().unwrap_or(&Vec::new()));
                let body_cost = self.estimate_block_cost(body);
                1 + body_cost + fallback_cost
            }
            Statement::Select { cases, timeout, .. } => {
                let case_max_cost = cases
                    .iter()
                    .map(|c| self.estimate_block_cost(&c.body))
                    .max()
                    .unwrap_or(0);
                let timeout_cost = timeout
                    .as_ref()
                    .map(|b| self.estimate_block_cost(b))
                    .unwrap_or(0);
                1 + case_max_cost.max(timeout_cost)
            }
            Statement::MatchEntropy {
                valid_branch,
                decayed_branch,
                pending_branch,
                consumed_branch,
                ..
            } => {
                let valid_cost = valid_branch
                    .as_ref()
                    .map(|(_, body)| self.estimate_block_cost(body))
                    .unwrap_or(0);
                let decayed_cost = decayed_branch
                    .as_ref()
                    .map(|(_, body)| self.estimate_block_cost(body))
                    .unwrap_or(0);
                let pending_cost = pending_branch
                    .as_ref()
                    .map(|body| self.estimate_block_cost(body))
                    .unwrap_or(0);
                let consumed_cost = consumed_branch
                    .as_ref()
                    .map(|body| self.estimate_block_cost(body))
                    .unwrap_or(0);
                1 + valid_cost
                    .max(decayed_cost)
                    .max(pending_cost)
                    .max(consumed_cost)
            }
            Statement::Collapse => 0,
            Statement::SplitMap { .. } => 1,
            Statement::Inspect { body, .. } => self.estimate_block_cost(body),
            Statement::Yield(_) => 0,
            Statement::RoutineDef { taking_ms, .. } => taking_ms.unwrap_or(0),
            Statement::Loop { max_ms, .. } => *max_ms,
            Statement::LoopTick { .. } => 1,
            Statement::Slice { .. } => 0,
            Statement::SpeculationMode(_) => 0,
            Statement::Await(_) => 1,
            Statement::Break => 0,
            Statement::Entangle { .. } => 0,
        };

        base + extra
    }
}
