use alloc::{collections::BTreeMap, vec, vec::Vec};
use spin::lock_api::Mutex;

use crate::scheduler::{block_thread_drop, current_thread, thread::ThreadId, unblock_thread};

#[derive(Debug)]
pub struct WaitMap<T: Ord> {
    tree: Mutex<BTreeMap<Option<T>, Vec<ThreadId>>>,
}

impl<T: Ord> WaitMap<T> {
    pub fn new() -> Self {
        Self {
            tree: Mutex::new(BTreeMap::new()),
        }
    }

    /// Unpause all the threads that are waiting for `val` or for any value.
    pub fn send(&self, val: T) {
        let mut tree = self.tree.lock();
        let val_threads = tree.remove(&Some(val));
        let any_threads = tree.remove(&None);
        drop(tree);

        if let Some(threads) = val_threads {
            for thread in threads {
                unblock_thread(thread).unwrap();
            }
        }
        if let Some(threads) = any_threads {
            for thread in threads {
                unblock_thread(thread).unwrap();
            }
        }
    }

    fn wait_key<D>(&self, key: Option<T>, drop: D) {
        let current_id = current_thread().id();
        let mut tree = self.tree.lock();
        if let Some(threads) = tree.get_mut(&key) {
            threads.push(current_id);
        } else {
            tree.insert(key, vec![current_id]);
        }

        block_thread_drop((tree, drop));
    }

    #[inline]
    /// Pause the current thread while waiting for another thread to send an equal `val`.
    pub fn wait(&self, val: T) {
        self.wait_key(Some(val), ());
    }

    #[inline]
    /// Pause the current thread while waiting for another thread to send something.
    pub fn wait_any(&self) {
        self.wait_key(None, ());
    }

    #[inline]
    /// Same as `wait` except that take a value to drop. See [block_thread_drop](crate::scheduler::block_thread_drop) for more info.
    pub fn wait_drop<D>(&self, val: T, drop: D) {
        self.wait_key(Some(val), drop);
    }

    #[inline]
    /// Same as `wait_any` except that take a value to drop. See [block_thread_drop](crate::scheduler::block_thread_drop) for more info.
    pub fn wait_any_drop<D>(&self, drop: D) {
        self.wait_key(None, drop);
    }
}
