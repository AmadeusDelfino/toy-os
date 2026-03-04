#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub fn new(foreground: Color, background: Color) -> ColorCode {
        // 16 colors fit in 4 bits (0000..1111), so the background shifts to the upper nibble
        // Example: background 0000_1111, foreground 0000_0001
        // 0000_1111 << 4 = 1111_0000
        // 1111_0000 | 0000_0001 = 1111_0001
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}
