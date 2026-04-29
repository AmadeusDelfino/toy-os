//! Fixed-size block allocator used as the kernel heap allocator.
//!
//! This allocator serves small allocations from segregated free lists and uses
//! `linked_list_allocator::Heap` as a fallback for larger or more strictly
//! aligned layouts. It is intentionally simple, but the rest of the kernel must
//! respect the contracts below for it to remain predictable. Simple, but nice :)
//!
//! ## Allocation policy
//!
//! - A layout is classified by `max(layout.size(), layout.align())`.
//! - Layouts that fit in one of `BLOCK_SIZES` are served by the smallest bucket
//!   whose block size is greater than or equal to that required size.
//! - Layouts larger than the largest bucket, or requiring stronger alignment,
//!   are served directly by the fallback heap.
//! - When a bucket is empty, it is refilled by allocating one fresh block from
//!   the fallback heap using `Layout { size: block_size, align: block_size }`.
//! - Once a block has been assigned to a bucket, freeing it returns it to that
//!   bucket's free list. Bucket blocks are not returned to the fallback heap (I don't
//!   wanna suffer with manual allocation now).
//!
//! ## Caller contract
//!
//! The `GlobalAlloc` contract is part of this allocator's safety boundary:
//!
//! - `dealloc` must receive the same `Layout` that was used for `alloc`.
//! - `dealloc` must not receive null pointers.
//! - Pointers passed to `dealloc` must have been returned by this allocator and
//!   must not already have been freed.
//! - Violating those requirements can corrupt the intrusive free lists. The
//!   allocator does not currently detect double-free, invalid pointers, or
//!   mismatched layouts.
//!
//! ## Initialization contract
//!
//! `init` must be called exactly once, after the heap virtual memory range has
//! been mapped and before any heap allocation is performed. Re-initializing the
//! fallback heap over memory that may already contain allocations would make the
//! allocator state invalid.
//!
//! ## Concurrency and interrupt contract
//!
//! The allocator is installed through `Locked<FixedSizeBlockAllocator>`, so
//! normal access is serialized by the global lock. The lock is a `spin::Mutex`;
//! it does not disable interrupts. Interrupt handlers must not allocate while
//! the kernel follows the current locking policy, because an interrupt that
//! re-enters the allocator while interrupted code holds the lock can deadlock.
//!
//! If future kernel code needs allocation from interrupt context, the locking
//! policy or allocator design must be changed deliberately before that use.

use super::Locked;
use crate::println;
use alloc::alloc::{GlobalAlloc, Layout};
use core::mem::{align_of, size_of};
use core::ptr::{self, NonNull};

const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SizeClass {
    Bucket { index: usize, block_size: usize },
    Fallback,
}

fn classify_layout(layout: &Layout) -> SizeClass {
    let required_block_size = layout.size().max(layout.align());
    match BLOCK_SIZES
        .iter()
        .enumerate()
        .find(|&(_, &block_size)| block_size >= required_block_size)
    {
        Some((index, &block_size)) => SizeClass::Bucket { index, block_size },
        None => SizeClass::Fallback,
    }
}

struct ListNode {
    next: *mut ListNode,
}

const fn is_power_of_two(value: usize) -> bool {
    value != 0 && (value & (value - 1)) == 0
}

const fn block_size_invariants_hold() -> bool {
    if BLOCK_SIZES.is_empty() {
        return false;
    }

    if BLOCK_SIZES[0] < size_of::<ListNode>() {
        return false;
    }

    if BLOCK_SIZES[0] < align_of::<ListNode>() {
        return false;
    }

    let mut index = 0;
    while index < BLOCK_SIZES.len() {
        let block_size = BLOCK_SIZES[index];

        if !is_power_of_two(block_size) {
            return false;
        }

        if index > 0 && BLOCK_SIZES[index - 1] >= block_size {
            return false;
        }

        index += 1;
    }

    true
}

const _: () = assert!(block_size_invariants_hold());

pub struct FixedSizeBlockAllocator {
    list_heads: [*mut ListNode; BLOCK_SIZES.len()],
    fallback_allocator: linked_list_allocator::Heap,
}

impl FixedSizeBlockAllocator {
    pub const fn new() -> Self {
        FixedSizeBlockAllocator {
            list_heads: [ptr::null_mut(); BLOCK_SIZES.len()],
            fallback_allocator: linked_list_allocator::Heap::empty(),
        }
    }

    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        unsafe {
            self.fallback_allocator.init(heap_start, heap_size);
        }
    }

    fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
        match self.fallback_allocator.allocate_first_fit(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => ptr::null_mut(),
        }
    }

    pub fn list_blocks(&self) {
        for (i, &head) in self.list_heads.iter().enumerate() {
            println!("bucket[{}] ({}B): ", i, BLOCK_SIZES[i]);
            let mut current = head;
            let mut count = 0;
            while !current.is_null() {
                count += 1;
                current = unsafe { (*current).next };
            }
            println!("{} blocks", count);
        }
    }
}

unsafe impl GlobalAlloc for Locked<FixedSizeBlockAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.lock();
        match classify_layout(&layout) {
            SizeClass::Bucket { index, block_size } => {
                let head = allocator.list_heads[index];
                if !head.is_null() {
                    allocator.list_heads[index] = unsafe { (*head).next };
                    head as *mut u8
                } else {
                    let layout = Layout::from_size_align(block_size, block_size).unwrap();
                    allocator.fallback_alloc(layout)
                }
            }
            SizeClass::Fallback => allocator.fallback_alloc(layout),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();
        match classify_layout(&layout) {
            SizeClass::Bucket { index, .. } => {
                let node_ptr = ptr as *mut ListNode;
                unsafe {
                    (*node_ptr).next = allocator.list_heads[index];
                    allocator.list_heads[index] = node_ptr;
                }
            }
            SizeClass::Fallback => {
                let ptr = NonNull::new(ptr).unwrap();
                unsafe {
                    allocator.fallback_allocator.deallocate(ptr, layout);
                }
            }
        }
    }
}

unsafe impl Send for FixedSizeBlockAllocator {}
unsafe impl Sync for FixedSizeBlockAllocator {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn block_size_table_satisfies_allocator_invariants() {
        assert!(block_size_invariants_hold());
    }

    #[test_case]
    fn classify_layout_returns_bucket_with_index_and_block_size() {
        assert_eq!(
            classify_layout(&Layout::from_size_align(1, 1).unwrap()),
            SizeClass::Bucket {
                index: 0,
                block_size: 8
            }
        );
        assert_eq!(
            classify_layout(&Layout::from_size_align(8, 8).unwrap()),
            SizeClass::Bucket {
                index: 0,
                block_size: 8
            }
        );
        assert_eq!(
            classify_layout(&Layout::from_size_align(9, 1).unwrap()),
            SizeClass::Bucket {
                index: 1,
                block_size: 16
            }
        );
        assert_eq!(
            classify_layout(&Layout::from_size_align(4, 32).unwrap()),
            SizeClass::Bucket {
                index: 2,
                block_size: 32
            }
        );
    }

    #[test_case]
    fn classify_layout_returns_fallback_when_no_bucket_fits() {
        assert_eq!(
            classify_layout(&Layout::from_size_align(2049, 1).unwrap()),
            SizeClass::Fallback
        );
        assert_eq!(
            classify_layout(&Layout::from_size_align(4, 4096).unwrap()),
            SizeClass::Fallback
        );
    }
}
