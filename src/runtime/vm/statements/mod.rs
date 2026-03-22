use crate::frontend::ast::SpannedStatement;
use crate::runtime::vm::error::TemporalError;
use crate::runtime::vm::state::Vm;

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
