use super::Locked;
use crate::println;
use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr::{self, NonNull};

const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

fn list_index(layout: &Layout) -> Option<usize> {
    let required_block_size = layout.size().max(layout.align());
    BLOCK_SIZES.iter().position(|&s| s >= required_block_size)
}

struct ListNode {
    next: *mut ListNode,
}

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
        match list_index(&layout) {
            Some(index) => {
                let head = allocator.list_heads[index];
                if !head.is_null() {
                    allocator.list_heads[index] = unsafe { (*head).next };
                    head as *mut u8
                } else {
                    let block_size = BLOCK_SIZES[index];
                    let layout = Layout::from_size_align(block_size, block_size).unwrap();
                    allocator.fallback_alloc(layout)
                }
            }
            None => allocator.fallback_alloc(layout),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();
        match list_index(&layout) {
            Some(index) => {
                let node_ptr = ptr as *mut ListNode;
                unsafe {
                    (*node_ptr).next = allocator.list_heads[index];
                    allocator.list_heads[index] = node_ptr;
                }
            }
            None => {
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