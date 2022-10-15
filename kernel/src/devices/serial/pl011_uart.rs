use core::fmt::Write;

#[derive(Debug)]
pub struct Pl011 {
    base: usize,
}

impl Pl011 {
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    #[inline]
    fn output_byte(&mut self, byte: u8) {
        unsafe {
            *(self.base as *mut u8) = byte;
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
