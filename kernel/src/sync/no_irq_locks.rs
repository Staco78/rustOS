use lock_api::{GuardSend, RawMutex, RawRwLock};
use spin::{self, Mutex, RwLock};

use crate::interrupts::exceptions::{disable_exceptions_depth, restore_exceptions_depth};

pub type NoIrqMutex<T> = lock_api::Mutex<NoIrqMutexRaw, T>;
pub type NoIrqMutexGuard<'a, T> = lock_api::MutexGuard<'a, NoIrqMutexRaw, T>;

pub struct NoIrqMutexRaw<R: RawMutex = Mutex<()>>(R);

unsafe impl<R: RawMutex> RawMutex for NoIrqMutexRaw<R> {
    type GuardMarker = GuardSend;

    const INIT: Self = Self(R::INIT);

    #[inline(always)]
    fn lock(&self) {
        disable_exceptions_depth();
        self.0.lock();
    }

    #[inline(always)]
    fn try_lock(&self) -> bool {
        disable_exceptions_depth();
        match self.0.try_lock() {
            true => true,
            false => {
                restore_exceptions_depth();
                false
            }
        }
    }

    #[inline(always)]
    unsafe fn unlock(&self) {
        self.0.unlock();
        restore_exceptions_depth();
    }
}

pub type NoIrqRwLock<T> = lock_api::RwLock<NoIrqRwLockRaw, T>;
pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, NoIrqRwLockRaw, T>;
pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, NoIrqRwLockRaw, T>;

pub struct NoIrqRwLockRaw<R: RawRwLock = RwLock<()>>(R);

unsafe impl<R: RawRwLock> RawRwLock for NoIrqRwLockRaw<R> {
    const INIT: Self = Self(R::INIT);
    type GuardMarker = GuardSend;

    #[inline(always)]
    fn lock_shared(&self) {
        disable_exceptions_depth();
        self.0.lock_shared()
    }

    #[inline(always)]
    fn try_lock_shared(&self) -> bool {
        disable_exceptions_depth();
        match self.0.try_lock_shared() {
            true => true,
            false => {
                restore_exceptions_depth();
                false
            }
        }
    }

    #[inline(always)]
    unsafe fn unlock_shared(&self) {
        self.0.unlock_shared();
        restore_exceptions_depth();
    }

    #[inline(always)]
    fn lock_exclusive(&self) {
        disable_exceptions_depth();
        self.0.lock_exclusive()
    }

    #[inline(always)]
    fn try_lock_exclusive(&self) -> bool {
        disable_exceptions_depth();
        match self.0.try_lock_exclusive() {
            true => true,
            false => {
                restore_exceptions_depth();
                false
            }
        }
    }

    #[inline(always)]
    unsafe fn unlock_exclusive(&self) {
        self.0.unlock_exclusive();
        restore_exceptions_depth();
    }

    #[inline(always)]
    fn is_locked(&self) -> bool {
        self.0.is_locked()
    }

    #[inline(always)]
    fn is_locked_exclusive(&self) -> bool {
        self.0.is_locked_exclusive()
    }
}
