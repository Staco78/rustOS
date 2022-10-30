use core::{sync::atomic::{AtomicUsize, Ordering}, fmt::Debug};

use alloc::vec::Vec;

use crate::memory::AddrSpaceLock;

use super::{sync_ref::SyncRef, thread::ThreadRef};

pub type ProcessId = usize;
pub type ProcessRef = SyncRef<Process>;

static PROCESS_ID: AtomicUsize = AtomicUsize::new(0);

#[inline]
fn get_next_id() -> ProcessId {
    PROCESS_ID.fetch_add(1, Ordering::Relaxed) as ProcessId
}

#[derive(Debug)]
pub struct Process {
    id: ProcessId,
    pub threads: Vec<ThreadRef>,

    pub addr_space: AddrSpaceLock,
}

impl Process {
    // this does not alloc
    pub fn new(addr_space: AddrSpaceLock) -> Self {
        Self {
            id: get_next_id(),
            threads: Vec::new(),
            addr_space,
        }
    }

    #[inline]
    pub fn id(&self) -> usize {
        self.id
    }

    #[inline]
    // this alloc
    pub fn into_ref(self) -> ProcessRef {
        ProcessRef::new(self)
    }

    #[inline]
    pub fn add_thread(&mut self, thread: ThreadRef) {
        self.threads.push(thread);
    }
}

impl ProcessRef {
    #[inline]
    pub fn id(&self) -> ProcessId {
        let ptr = self.data_ptr();
        unsafe { (*ptr).id }
    }

    // this is safe because we have a lock in a lock
    pub fn get_addr_space(&self) -> &AddrSpaceLock {
        let ptr = self.data_ptr();
        unsafe { &(*ptr).addr_space }
    }
}