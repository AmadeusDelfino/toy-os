mod fixed_size_block;
mod heap;

use crate::lock::Locked;
use fixed_size_block::FixedSizeBlockAllocator;
pub use heap::init_heap;

#[global_allocator]
static ALLOCATOR: Locked<FixedSizeBlockAllocator> = Locked::new(FixedSizeBlockAllocator::new());

/// Prints the current fixed-size bucket free-list counts.
///
/// This is the intentional diagnostic entry point for allocator internals. Code
/// outside this module should not lock the global allocator directly.
pub fn debug_print_free_lists() {
    ALLOCATOR.lock().list_blocks();
}
