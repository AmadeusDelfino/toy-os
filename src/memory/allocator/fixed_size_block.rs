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
//! - If the fallback heap cannot satisfy a request, or if allocation is
//!   attempted before heap initialization, `alloc` returns null. That is the
//!   expected `GlobalAlloc` OOM/pre-init signal; it is not memory corruption by
//!   itself.
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
//! been mapped and before any heap allocation is performed. Until that happens,
//! allocation requests return null. Re-initializing the fallback heap over
//! memory that may already contain allocations would make the allocator state
//! invalid, so a second initialization is rejected.
//!
//! Kernel boot code must not create `Box`, `Vec`, `Arc`, heap-backed async
//! tasks, dynamic queues, or any other heap-allocated object before
//! `init_heap` returns `Ok(())`.
//!
//! ## Concurrency and interrupt contract
//!
//! The allocator is installed through `Locked<FixedSizeBlockAllocator>`, so
//! normal access is serialized by the global lock. The lock is a `spin::Mutex`;
//! it does not disable interrupts. Normal external IRQ handlers must not
//! allocate while the kernel follows the current locking policy, because an IRQ
//! that re-enters the allocator while interrupted code holds the lock can
//! deadlock.
//!
//! Exception and fault handlers are terminal diagnostic paths and are outside
//! this normal-IRQ policy. If future kernel code needs allocation from normal
//! IRQ context, the locking policy or allocator design must be changed
//! deliberately before that use.
//!
//! ## Diagnostics contract
//!
//! Free-list diagnostics are exposed through
//! `memory::allocator::debug_print_free_lists`, which acquires the global
//! allocator lock before inspecting allocator internals. The diagnostic path
//! separates collection from printing, does not allocate, and bounds traversal
//! by bucket so a corrupted or cyclic free list is reported instead of looping
//! forever.

use super::Locked;
use crate::memory::HEAP_SIZE;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FreeListCount {
    Complete(usize),
    ExceededLimit { traversed: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BucketDiagnostics {
    bucket_index: usize,
    block_size: usize,
    count: FreeListCount,
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

fn free_list_traversal_limit(block_size: usize) -> usize {
    HEAP_SIZE / block_size + 1
}

fn count_free_list(mut current: *mut ListNode, limit: usize) -> FreeListCount {
    let mut traversed = 0;

    while !current.is_null() {
        if traversed >= limit {
            return FreeListCount::ExceededLimit { traversed };
        }

        traversed += 1;
        // SAFETY: Diagnostic traversal reads the intrusive free-list links.
        // Callers must pass a free-list head from allocator state while the
        // allocator lock is held, or a test-owned list.
        current = unsafe { (*current).next };
    }

    FreeListCount::Complete(traversed)
}

const _: () = assert!(block_size_invariants_hold());

pub struct FixedSizeBlockAllocator {
    list_heads: [*mut ListNode; BLOCK_SIZES.len()],
    fallback_allocator: linked_list_allocator::Heap,
    initialized: bool,
}

impl FixedSizeBlockAllocator {
    pub const fn new() -> Self {
        FixedSizeBlockAllocator {
            list_heads: [ptr::null_mut(); BLOCK_SIZES.len()],
            fallback_allocator: linked_list_allocator::Heap::empty(),
            initialized: false,
        }
    }

    pub(super) fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        assert!(
            !self.initialized,
            "heap allocator must not be initialized twice"
        );

        // SAFETY: The caller must provide a mapped, unused heap region and
        // call this exactly once before any heap allocations are performed.
        unsafe {
            self.fallback_allocator.init(heap_start, heap_size);
        }
        self.initialized = true;
    }

    fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
        match self.fallback_allocator.allocate_first_fit(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => ptr::null_mut(),
        }
    }

    fn collect_free_list_diagnostics(&self) -> [BucketDiagnostics; BLOCK_SIZES.len()] {
        core::array::from_fn(|index| {
            let block_size = BLOCK_SIZES[index];
            BucketDiagnostics {
                bucket_index: index,
                block_size,
                count: count_free_list(
                    self.list_heads[index],
                    free_list_traversal_limit(block_size),
                ),
            }
        })
    }

    pub(super) fn print_free_list_diagnostics(&self) {
        for diagnostic in self.collect_free_list_diagnostics() {
            match diagnostic.count {
                FreeListCount::Complete(free_blocks) => println!(
                    "bucket={} block_size={} free_blocks={} status=ok",
                    diagnostic.bucket_index, diagnostic.block_size, free_blocks
                ),
                FreeListCount::ExceededLimit { traversed } => println!(
                    "bucket={} block_size={} traversed={} status=possible_cycle_or_corruption",
                    diagnostic.bucket_index, diagnostic.block_size, traversed
                ),
            }
        }
    }
}

// SAFETY: `Locked<FixedSizeBlockAllocator>` serializes all allocator state
// access through its mutex. Each unsafe pointer operation below relies on the
// `GlobalAlloc` caller contract documented at the top of this module.
unsafe impl GlobalAlloc for Locked<FixedSizeBlockAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.lock();
        if !allocator.is_initialized() {
            return ptr::null_mut();
        }

        match classify_layout(&layout) {
            SizeClass::Bucket { index, block_size } => {
                let head = allocator.list_heads[index];
                if !head.is_null() {
                    // SAFETY: `head` is non-null and free-list entries are
                    // only created by `dealloc`, which writes a valid
                    // `ListNode` into bucket-sized storage.
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
                // SAFETY: The `GlobalAlloc` caller must pass a non-null
                // pointer allocated by this allocator with the same `Layout`.
                // The selected bucket guarantees enough size/alignment to
                // store `ListNode`; double-free or a mismatched layout would
                // violate the caller contract.
                unsafe {
                    (*node_ptr).next = allocator.list_heads[index];
                    allocator.list_heads[index] = node_ptr;
                }
            }
            SizeClass::Fallback => {
                let ptr = NonNull::new(ptr).unwrap();
                // SAFETY: Layouts classified as fallback are allocated and
                // deallocated directly through the fallback heap. The
                // `GlobalAlloc` caller must provide the matching layout.
                unsafe {
                    allocator.fallback_allocator.deallocate(ptr, layout);
                }
            }
        }
    }
}

// SAFETY: The allocator's mutable state is only accessed through
// `Locked<FixedSizeBlockAllocator>`, which serializes access with `spin::Mutex`.
unsafe impl Send for FixedSizeBlockAllocator {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn block_size_table_satisfies_allocator_invariants() {
        assert!(block_size_invariants_hold());
    }

    #[test_case]
    fn new_allocator_starts_uninitialized() {
        let allocator = FixedSizeBlockAllocator::new();

        assert!(!allocator.is_initialized());
    }

    #[test_case]
    fn allocation_before_init_returns_null() {
        let allocator = Locked::new(FixedSizeBlockAllocator::new());
        let layout = Layout::from_size_align(8, 8).unwrap();

        let ptr = unsafe { GlobalAlloc::alloc(&allocator, layout) };

        assert!(ptr.is_null());
    }

    #[test_case]
    fn init_marks_allocator_initialized() {
        #[repr(align(16))]
        #[allow(dead_code)]
        struct TestHeap([u8; 4096]);

        static mut TEST_HEAP: TestHeap = TestHeap([0; 4096]);

        let mut allocator = FixedSizeBlockAllocator::new();
        let heap_start = ptr::addr_of_mut!(TEST_HEAP) as *mut u8 as usize;

        unsafe {
            allocator.init(heap_start, 4096);
        }

        assert!(allocator.is_initialized());
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

    #[test_case]
    fn count_free_list_returns_zero_for_empty_list() {
        assert_eq!(
            count_free_list(ptr::null_mut(), 3),
            FreeListCount::Complete(0)
        );
    }

    #[test_case]
    fn count_free_list_counts_a_small_list() {
        let mut third = ListNode {
            next: ptr::null_mut(),
        };
        let mut second = ListNode {
            next: &mut third as *mut ListNode,
        };
        let mut first = ListNode {
            next: &mut second as *mut ListNode,
        };

        assert_eq!(
            count_free_list(&mut first as *mut ListNode, 4),
            FreeListCount::Complete(3)
        );
    }

    #[test_case]
    fn count_free_list_stops_when_limit_is_exceeded() {
        let mut first = ListNode {
            next: ptr::null_mut(),
        };
        let mut second = ListNode {
            next: &mut first as *mut ListNode,
        };
        first.next = &mut second as *mut ListNode;

        assert_eq!(
            count_free_list(&mut first as *mut ListNode, 2),
            FreeListCount::ExceededLimit { traversed: 2 }
        );
    }

    #[test_case]
    fn free_list_traversal_limit_is_derived_from_heap_size() {
        assert_eq!(
            free_list_traversal_limit(8),
            crate::memory::HEAP_SIZE / 8 + 1
        );
        assert_eq!(
            free_list_traversal_limit(2048),
            crate::memory::HEAP_SIZE / 2048 + 1
        );
    }

    #[test_case]
    fn collect_free_list_diagnostics_reports_empty_buckets() {
        let allocator = FixedSizeBlockAllocator::new();
        let diagnostics = allocator.collect_free_list_diagnostics();

        assert_eq!(diagnostics.len(), BLOCK_SIZES.len());

        for (index, diagnostic) in diagnostics.iter().enumerate() {
            assert_eq!(diagnostic.bucket_index, index);
            assert_eq!(diagnostic.block_size, BLOCK_SIZES[index]);
            assert_eq!(diagnostic.count, FreeListCount::Complete(0));
        }
    }
}
