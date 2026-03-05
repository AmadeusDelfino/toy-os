pub mod color;
pub mod writer;
pub mod buffer;

use lazy_static::lazy_static;
use spin::Mutex;
use core::fmt;
pub use color::*;
pub use writer::*;
pub use buffer::*;

const VGA_BUFFER_ADDRESS: u32 = 0xb8000;

lazy_static! {
    pub static ref DISPLAY: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(VGA_BUFFER_ADDRESS as *mut Buffer) },
    });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    DISPLAY.lock().write_fmt(args).unwrap();
}
