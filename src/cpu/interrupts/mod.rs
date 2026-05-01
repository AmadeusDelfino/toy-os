//! CPU exception and interrupt descriptor-table setup.
//!
//! ## External IRQ allocation policy
//!
//! Normal external IRQ handlers, such as timer, keyboard, and future driver
//! interrupts, must not allocate or block. The global heap allocator is guarded
//! by a `spin::Mutex`, which does not disable interrupts; allocating from an IRQ
//! can deadlock if the interrupted code already holds the allocator lock.
//!
//! IRQ handlers should hand work to preallocated/static structures, bounded
//! lock-free queues, or other explicitly non-allocating paths. Do not use
//! `Box`, `Vec`, `Arc`, `BTreeMap`, `String`, `format!`, or APIs that may
//! allocate from normal external IRQ context.
//!
//! Exception and fault handlers are terminal diagnostic paths in the current
//! kernel and are intentionally outside this normal-IRQ policy.

pub mod pic8259;
pub use self::pic8259::*;

use crate::cpu::tss;
use crate::println;
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.bound_range_exceeded.set_handler_fn(bound_range_exceeded_handler);
        idt.alignment_check.set_handler_fn(alignment_check_handler);
        idt.cp_protection_exception.set_handler_fn(cp_protection_exception_handler);

        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
                .set_stack_index(tss::DOUBLE_FAULT_IST_INDEX);
        }

        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);

        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame, _error_code: u64) -> !
{
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn general_protection_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic!("EXCEPTION: GENERAL PROTECTION FAULT\n{:#?}\n{:#?}", stack_frame, error_code);
}
extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error_code: PageFaultErrorCode) {
    panic!("EXCEPTION: PAGE FAULT\n {:#?}\n{:#?}", stack_frame, error_code);
}

extern "x86-interrupt" fn bound_range_exceeded_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: BOUND RANGE EXCEEDED\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn alignment_check_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic!("EXCEPTION: ALIGNMENT CHECK\n {:#?}\n{:#?}", stack_frame, error_code);
}

extern "x86-interrupt" fn cp_protection_exception_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic!("EXCEPTION: CP PROTECTION FAULT\n{:#?}\n{:#?}", stack_frame, error_code);
}

// external interrupts

extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    use crate::keyboard::scancode::add_scancode;

    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    add_scancode(scancode);

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}
