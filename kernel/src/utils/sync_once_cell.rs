use core::{cell::SyncUnsafeCell, fmt::Debug};

pub struct SyncOnceCell<T> {
    inner: SyncUnsafeCell<Option<T>>,
}

impl<T> SyncOnceCell<T> {
    pub const fn new() -> Self {
        Self {
            inner: SyncUnsafeCell::new(None),
        }
    }

    pub fn get(&self) -> Option<&T> {
        unsafe { &*self.inner.get() }.as_ref()
    }

    /// Safety: don't call this at same time on different threads
    pub unsafe fn set(&self, value: T) -> Result<(), T> {
        let slot = unsafe { &*self.inner.get() };
        if slot.is_some() {
            return Err(value);
        }

        let slot = unsafe { &mut *self.inner.get() };
        *slot = Some(value);
        Ok(())
    }
}

impl<T: Debug> Debug for SyncOnceCell<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let data = unsafe { &*self.inner.get() };
        f.debug_tuple("SyncOnceCell").field(data).finish()
    }
}
