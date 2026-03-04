#![no_std]
#![no_main]

pub mod vga_buffer;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}


#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    println!("Starting Toy-OS!");
    loop {}
}