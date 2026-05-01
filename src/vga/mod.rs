pub mod buffer;
pub mod color;
pub mod writer;

use crate::lock::LockedIrq;
pub use buffer::*;
pub use color::*;
use core::fmt;
use lazy_static::lazy_static;
pub use writer::*;

const VGA_BUFFER_ADDRESS: u32 = 0xb8000;

lazy_static! {
    pub static ref DISPLAY: LockedIrq<Writer> = LockedIrq::new(Writer {
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
    DISPLAY
        .lock()
        .write_fmt(args)
        .expect("Failed to write in display buffer");
}
