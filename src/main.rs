#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(toy_os::test_runner)]
#![reexport_test_harness_main = "test_main"]
extern crate alloc;

use alloc::sync::Arc;
use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;
use spin::Mutex;
use toy_os::{
    asynchronous::{Task, executor::Executor},
    cpu::{CPU, interrupts::apic::APIC},
    keyboard::print_keypresses,
    memory::{
        allocator::init_heap,
        frame::BootInfoFrameAllocator,
        init as memory_init,
        map_phys_frame_to_virt_page,
    },
    println,
};
use x86_64::{VirtAddr, structures::paging::PageTableFlags};

entry_point!(kernel_main);

async fn loop_initialized_log() {
    println!("Loop initialized!");
}

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    println!("Starting Toy-OS!");
    toy_os::init();

    #[cfg(test)]
    test_main();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut memory_mapper = unsafe { memory_init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };
    init_heap(&mut memory_mapper, &mut frame_allocator).expect("heap initialization failed");
    let cpu = CPU::new();

    cpu.brand.print();
    cpu.topology.print();

    if !cpu.topology.apic_supported {
        panic!("APIC not supported");
    }

    if cpu.topology.x2apic_enabled {
        panic!("x2APIC enabled. Kernel only supports xAPIC");
    }
    let apic_mem_page = map_phys_frame_to_virt_page(
        &mut memory_mapper,
        &mut frame_allocator,
        cpu.topology.get_apic_mmio_phys_address(),
        0x4444_0000_0000,
        PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::WRITE_THROUGH
            | PageTableFlags::NO_CACHE,
    );
    let apic = APIC::new(apic_mem_page);
    let lapic_version = apic.version();
    let lapic_id = apic.id();
    println!("lapic_version: {}", lapic_version);
    println!("lapic_id: {}", lapic_id);

    let mut executor = Executor::new();
    executor.spawn(Task::new(loop_initialized_log()));
    executor.spawn(Task::new(print_keypresses()));
    println!("Toy-OS started");
    println!("Initializing loop...");

    executor.run();
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);

    toy_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    toy_os::test_panic_handler(info)
}
