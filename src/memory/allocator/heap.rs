use super::ALLOCATOR;
use crate::memory::{HEAP_SIZE, HEAP_START};
use crate::println;
use x86_64::{
    VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, mapper::MapToError,
    },
};

/// Maps the kernel heap range and initializes the global heap allocator.
///
/// This must complete successfully before any code creates heap-backed values
/// such as `Box`, `Vec`, `Arc`, async tasks, or dynamic queues. Calling it more
/// than once is a boot-order bug and is rejected before attempting to remap heap
/// pages.
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    assert!(
        !ALLOCATOR.lock().is_initialized(),
        "heap allocator must not be initialized twice"
    );

    println!("Initializing heap allocation");
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        // SAFETY: The frame allocator returns an unused physical frame, and
        // this function maps each heap page exactly once before the heap
        // allocator is initialized.
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() };
    }

    // SAFETY: All pages in the heap range were mapped above. `init_heap` is the
    // single initialization path and must run before heap use.
    unsafe {
        ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }
    println!(
        "Heap allocation successfully initialized. Start: {} | Size: {}",
        HEAP_START, HEAP_SIZE
    );
    Ok(())
}
