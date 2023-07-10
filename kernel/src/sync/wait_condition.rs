use core::{mem, ops::DerefMut};

use alloc::vec::Vec;
use spin::lock_api::Mutex;

use crate::scheduler::{block_thread_drop, current_thread, thread::ThreadRef, unblock_thread};

#[derive(Debug)]
pub struct WaitCondition {
    waiters: Mutex<Vec<ThreadRef>>,
}

impl WaitCondition {
    pub const fn new() -> Self {
        Self {
            waiters: Mutex::new(Vec::new()),
        }
    }

    pub fn wait(&self) {
        let current_thread = current_thread().clone();
        let mut waiters = self.waiters.lock();
        waiters.push(current_thread);

        block_thread_drop(waiters);
    }

    pub fn notify_all(&self) {
        let mut waiters_lock = self.waiters.lock();
        let mut waiters = Vec::new();
        mem::swap(waiters_lock.deref_mut(), &mut waiters);
        drop(waiters_lock);
        for waiter in waiters {
            unblock_thread(waiter.id()).unwrap();
        }
    }

    pub fn notify_one(&self) {
        let mut waiters = self.waiters.lock();
        if waiters.first().is_some() {
            let waiter = waiters.swap_remove(0);
            drop(waiters);
            unblock_thread(waiter.id()).unwrap();
        }
    }
}
