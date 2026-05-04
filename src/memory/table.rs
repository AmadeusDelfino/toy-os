use x86_64::{
    PhysAddr,
    VirtAddr,
    structures::paging::{
        FrameAllocator,
        Mapper,
        OffsetPageTable,
        Page,
        PageTable,
        PageTableFlags,
        PhysFrame,
        Size4KiB,
    },
};

/// Initialize a new OffsetPageTable.
///
/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must be only called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    unsafe {
        let level_4_table = active_level_4_table(physical_memory_offset);
        OffsetPageTable::new(level_4_table, physical_memory_offset)
    }
}

/// Returns a mutable reference to the active level 4 table.
///
/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must be only called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    unsafe { &mut *page_table_ptr }
}

pub fn map_phys_frame_to_virt_page(
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    phys_address: u64,
    virt_address: u64,
    flags: PageTableFlags,
) -> VirtAddr {
    let frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(PhysAddr::new(phys_address));
    let virt_address = VirtAddr::new(virt_address);
    let page = Page::from_start_address(virt_address).unwrap();
    let map_to_result = unsafe { mapper.map_to(page, frame, flags, frame_allocator) };
    map_to_result.unwrap().flush();

    virt_address
}
