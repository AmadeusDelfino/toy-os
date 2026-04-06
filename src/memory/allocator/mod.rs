mod heap;
mod linked_list;
mod bump;

use crate::lock::Locked;
use crate::memory::allocator::linked_list::LinkedListAllocator;
pub use heap::init_heap;

#[global_allocator]
static ALLOCATOR: Locked<LinkedListAllocator> =
    Locked::new(LinkedListAllocator::new());

