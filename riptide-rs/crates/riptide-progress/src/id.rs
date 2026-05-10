//! ID newtypes and allocator for progress entities.

use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for a top-level download operation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct OperationId(u64);

/// Unique identifier for a group within an operation (e.g. an album inside an
/// artist download).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GroupId(u64);

/// Unique identifier for a single download item (track, local file).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ItemId(u64);

/// Allocates unique IDs within a process.
///
/// IDs are unique within their own namespace — a [`GroupId`] and an [`ItemId`]
/// may share the same numeric value. The newtypes prevent accidental mixing.
/// Tests must not assert stable numeric values unless they allocate explicitly.
#[allow(clippy::struct_field_names)]
pub struct ProgressIdAllocator {
    next_operation: AtomicU64,
    next_group: AtomicU64,
    next_item: AtomicU64,
}

impl Default for ProgressIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressIdAllocator {
    /// Creates a new allocator. IDs start at 1.
    pub const fn new() -> Self {
        Self {
            next_operation: AtomicU64::new(1),
            next_group: AtomicU64::new(1),
            next_item: AtomicU64::new(1),
        }
    }

    /// Allocates the next [`OperationId`].
    pub fn next_operation_id(&self) -> OperationId {
        OperationId(self.next_operation.fetch_add(1, Ordering::Relaxed))
    }

    /// Allocates the next [`GroupId`].
    pub fn next_group_id(&self) -> GroupId {
        GroupId(self.next_group.fetch_add(1, Ordering::Relaxed))
    }

    /// Allocates the next [`ItemId`].
    pub fn next_item_id(&self) -> ItemId {
        ItemId(self.next_item.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_ids_are_unique_per_namespace() {
        let alloc = ProgressIdAllocator::new();
        let op1 = alloc.next_operation_id();
        let op2 = alloc.next_operation_id();
        assert_ne!(op1, op2);

        let g1 = alloc.next_group_id();
        let g2 = alloc.next_group_id();
        assert_ne!(g1, g2);

        let i1 = alloc.next_item_id();
        let i2 = alloc.next_item_id();
        assert_ne!(i1, i2);
    }

    #[test]
    fn newtypes_prevent_id_confusion() {
        // This test exists to document the intent; if it compiled with mixed types
        // it would be a type-system regression.
        let alloc = ProgressIdAllocator::new();
        let _op: OperationId = alloc.next_operation_id();
        let _gr: GroupId = alloc.next_group_id();
        let _it: ItemId = alloc.next_item_id();
    }
}
