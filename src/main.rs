#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(toy_os::test_runner)]
#![reexport_test_harness_main = "test_main"]
extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use toy_os::asynchronous::executor::Executor;
use toy_os::asynchronous::Task;
use toy_os::cpu::info::{CpuBrandString, CpuTopology};
use toy_os::keyboard::print_keypresses;
use toy_os::memory::allocator::{init_heap, ALLOCATOR};
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

    if let Some(cpu_brand) = CpuBrandString::read() {
        println!("CPU brand: {}", cpu_brand.as_str());
    } else {
        println!("WARNING: failed to parse cpu brandstring");
    }

    if let Some(cpu_topology) = CpuTopology::read() {
        println!("CPU topology. Cores ({}) | Threads ({})", cpu_topology.cores, cpu_topology.logical_processors);
    } else {
        println!("WARNING: failed to read cpu topology");
    }
    {
        let a = Box::new(42);
        let b = Box::new(42);
        let c = Box::new(4200000);
        let d = vec![a, b, c];
        let e = vec![d];
        let f = vec![0..10000];
    }

    ALLOCATOR.lock().list_blocks();

    println!("Toy-OS started");
    let mut executor = Executor::new();
    executor.spawn(Task::new(print_keypresses()));
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
