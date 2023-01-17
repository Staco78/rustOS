use core::{fmt::Write, ptr};

use crate::memory::VirtualAddress;

#[derive(Debug)]
pub struct Pl011 {
    base: VirtualAddress,
}

impl Pl011 {
    pub const fn new(base: VirtualAddress) -> Self {
        Self { base }
    }

    #[inline]
    fn output_byte(&mut self, byte: u8) {
        unsafe {
            ptr::write_volatile(self.base.as_ptr(), byte);
        }
    }
}

impl Write for Pl011 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            self.write_char(c)?
        }
        Ok(())
    }

    fn write_char(&mut self, c: char) -> core::fmt::Result {
        let mut b = [0; 4];
        c.encode_utf8(&mut b);

        for c in b {
            if c != 0 {
                self.output_byte(c);
            }
        }

        if c == '\n' {
            self.output_byte(b'\r');
        }

        Ok(())
    }
}
