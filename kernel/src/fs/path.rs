use core::ops::Deref;

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub struct Path {
    inner: str,
}

impl Path {
    pub fn new(str: &str) -> &Self {
        unsafe { &*(str as *const str as *const Self) }
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn is_absolute(&self) -> bool {
        if let Some(first) = self.inner.chars().nth(0) {
            first == '/'
        } else {
            false
        }
    }

    pub fn components<'a>(&'a self) -> impl DoubleEndedIterator<Item = &str> {
        self.inner.split('/')
    }

    pub fn file_name(&self) -> Option<&str> {
        self.components()
            .next_back()
            .and_then(|p| if p == "" { None } else { Some(p) })
    }
}

impl Deref for Path {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> From<&'a str> for &'a Path {
    fn from(value: &'a str) -> Self {
        Path::new(value)
    }
}

impl AsRef<Path> for &str {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}
