#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![test_runner(toy_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;
use toy_os::{exit_qemu, serial_print, serial_println, QemuExitCode};
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    test_main();

    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    toy_os::test_panic_handler(info)
}

lazy_static! {
    static ref TEST_IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        unsafe {
            idt.double_fault
                .set_handler_fn(test_double_fault_handler)
                .set_stack_index(toy_os::cpu::tss::DOUBLE_FAULT_IST_INDEX);        }

        idt
    };
}

extern "x86-interrupt" fn test_double_fault_handler(
    _stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    serial_println!("[ok]");
    exit_qemu(QemuExitCode::Success);
    loop {}
}

pub fn init_test_idt() {
    TEST_IDT.load();
}

#[allow(unconditional_recursion)]
fn stack_overflow() {
    stack_overflow(); // for each recursion, the return address is pushed
    volatile::Volatile::new(0).read(); // prevent tail recursion optimizations
}

#[test_case]
fn stack_overflow_test() {
    serial_print!("stack_overflow::stack_overflow...\t");

    toy_os::cpu::gdt::init_gdt();
    init_test_idt();
    // trigger a stack overflow
    stack_overflow();

    panic!("Execution continued after stack overflow");
}