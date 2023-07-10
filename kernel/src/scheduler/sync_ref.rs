use core::fmt::Debug;

use alloc::sync::Arc;

use crate::sync::no_irq_locks::{NoIrqRwLock, RwLockReadGuard, RwLockWriteGuard};

pub struct SyncRef<T>(Arc<NoIrqRwLock<T>>);

impl<T> SyncRef<T> {
    pub fn new(data: T) -> Self {
        Self(Arc::new(NoIrqRwLock::new(data)))
    }

    #[inline]
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        self.0.read()
    }

    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.0.write()
    }

    #[inline]
    pub fn data_ptr(&self) -> *mut T {
        self.0.data_ptr()
    }
}

impl<T: Debug> Debug for SyncRef<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        unsafe { write!(f, "SyncRef({:#?})", *self.data_ptr()) }
    }
}

impl<T> Clone for SyncRef<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

unsafe impl<T> Send for SyncRef<T> {}
unsafe impl<T> Sync for SyncRef<T> {}
