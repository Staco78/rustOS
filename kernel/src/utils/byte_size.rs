use core::fmt::Display;

#[derive(Debug)]
pub struct ByteSize(pub usize);

impl Display for ByteSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0 < 1024 {
            write!(f, "{} B", self.0)
        } else {
            let s = self.0 as f64;
            if self.0 < 1024 * 1024 {
                write!(f, "{:.2} KB", s / 1024.)
            } else if self.0 < 1024 * 1024 * 1024 {
                write!(f, "{:.2} MB", s / 1024. / 1024.)
            } else {
                write!(f, "{:.2} GB", s / 1024. / 1024. / 1024.)
            }
        }
    }
}
