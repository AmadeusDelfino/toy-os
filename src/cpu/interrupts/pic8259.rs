use crate::lock::LockedIrq;
use pic8259::ChainedPics;

// Here we set the offset to 32, as it is the first available slot in the IDT after exception interrupts.
pub const PIC_1_OFFSET: u8 = 32;
// Each PIC occupies 8 interrupt slots, so for the second current chip we increase its offset by 8.
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

// Here is a piece of code that is "unsafe safe".
// It's necessary to use `unsafe` because `ChainedPics::new` with incorrect offsets can generate a `UB` error,
// but in our case we are confident that we are using the correct offsets (or so we hope).
pub static PICS: LockedIrq<ChainedPics> =
    LockedIrq::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard, //33
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}
