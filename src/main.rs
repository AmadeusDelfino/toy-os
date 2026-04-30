#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(toy_os::test_runner)]
#![reexport_test_harness_main = "test_main"]
extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use toy_os::{
    asynchronous::{
        Task, executor::Executor
    },
    cpu::info::{
        CpuBrandString, CpuTopology
    },
    keyboard::print_keypresses,
    memory::{
        allocator::init_heap, frame::BootInfoFrameAllocator, init as memory_init
    },
    println
};
use x86_64::VirtAddr;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    println!("Starting Toy-OS!");
    toy_os::init();

    #[cfg(test)]
    test_main();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut memory_mapper = unsafe { memory_init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(&boot_info.memory_map)
    };
    init_heap(&mut memory_mapper, &mut frame_allocator).expect("heap initialization failed");

    CpuTopology::read().expect("cpu topology read failed").print();
    CpuBrandString::read().expect("cpu brand string read failed").print();

    let mut executor = Executor::new();
    executor.spawn(Task::new(print_keypresses()));
    println!("Toy-OS started");

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
