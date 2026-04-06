#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(toy_os::test_runner)]
#![reexport_test_harness_main = "test_main"]
extern crate alloc;


use alloc::boxed::Box;
use alloc::vec::Vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use toy_os::memory::allocator::init_heap;
use toy_os::memory::frame::BootInfoFrameAllocator;
use toy_os::{memory, println};
use x86_64::VirtAddr;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    println!("Starting Toy-OS!");
    toy_os::init();

    #[cfg(test)]
    test_main();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(&boot_info.memory_map)
    };

    init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");


    println!("Toy-OS started");

    let a = Box::new(42);
    let b = Vec::from([1, 2, 3, 4]);
    println!("{:p}", a);
    println!("{:?}", b);

    toy_os::hlt_loop();
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
