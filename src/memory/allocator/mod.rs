mod heap;
mod linked_list;
mod bump;
mod fixed_size_block;

use crate::lock::Locked;
use fixed_size_block::FixedSizeBlockAllocator;
pub use heap::init_heap;


#[global_allocator]
static ALLOCATOR: Locked<FixedSizeBlockAllocator> = Locked::new(
    FixedSizeBlockAllocator::new());
