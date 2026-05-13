use crate::vm::error::TemporalError;
use crate::vm::state::Vm;
use ictl_core::SpannedStatement;

mod handlers;

impl Vm {
    pub(crate) fn execute_statement_inner(
        &mut self,
        branch_id: &str,
        stmt: &SpannedStatement,
    ) -> Result<(), TemporalError> {
        handlers::execute_statement_inner(self, branch_id, stmt)
    }
}
