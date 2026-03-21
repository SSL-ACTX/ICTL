use crate::runtime::vm::Timeline;

/// P14: Entropic Garbage Collection
///
/// This module provides explicit collection primitives for branch timelines.
/// When a branch is removed from the VM, the associated arena is dropped
/// and host memory is reclaimed.
#[allow(dead_code)]
pub struct GarbageCollector;

impl GarbageCollector {
    /// Reclaim a completed or terminated timeline branch.
    pub fn collect_branch(mut branch: Timeline) {
        branch.clear_anchor_snapshots();
        branch.clear_arena();
        drop(branch);
    }

    /// Reclaim anchor snapshots when timeline crosses commit horizon.
    pub fn collect_commit_anchors(branch: &mut Timeline) {
        branch.clear_anchor_snapshots();
    }

    /// Reclaim a branch from the VM and remove it from active set.
    pub fn collect_branch_by_id(vm: &mut crate::runtime::vm::Vm, branch_id: &str) {
        if let Some(branch) = vm.active_branches.remove(branch_id) {
            Self::collect_branch(branch);
        }
    }
}
