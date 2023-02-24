use core::cell::SyncUnsafeCell;

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
        // Safety: Safe because we cannot have overlapping mutable borrows
        let slot = unsafe { &*self.inner.get() };
        if slot.is_some() {
            return Err(value);
        }

        // Safety: This is the only place where we set the slot, no races
        // due to reentrancy/concurrency are possible, and we've
        // checked that slot is currently `None`, so this write
        // maintains the `inner`'s invariant.
        let slot = unsafe { &mut *self.inner.get() };
        *slot = Some(value);
        Ok(())
    }
}
