use core::{
    fmt::Display,
    ops::{Deref, DerefMut},
    slice,
};

use spin::lock_api::{Mutex, MutexGuard};

use super::{
    mmu::TableEntry,
    vmm::{self, phys_to_virt},
    PageAllocator, ENTRIES_IN_TABLE, PAGE_SIZE, PMM_PAGE_ALLOCATOR,
};

#[derive(Debug)]
pub struct VirtualAddressSpace {
    pub ptr: *mut TableEntry, // the first table
    pub is_user: bool,        // TTBR0 or TTBR1 (before or after hole)
}

impl VirtualAddressSpace {
    pub unsafe fn new(ptr: *mut TableEntry, is_user: bool) -> Self {
        debug_assert!(ptr.addr() != 0);
        let ptr = phys_to_virt(ptr as usize) as *mut TableEntry;
        Self { ptr, is_user }
    }

    // return None if out of memory
    pub fn create_user() -> Option<Self> {
        let l1 = unsafe { PMM_PAGE_ALLOCATOR.get().unwrap().alloc(1) };
        if l1.is_null() {
            return None;
        }
        let ptr = phys_to_virt(l1.addr()) as *mut u8;
        unsafe { ptr.write_bytes(0, PAGE_SIZE) };

        Some(unsafe { Self::new(l1.addr() as *mut _, true) })
    }

    #[inline]
    pub fn get_table(&self) -> &'static [TableEntry] {
        unsafe { slice::from_raw_parts(self.ptr, ENTRIES_IN_TABLE) }
    }

    #[inline]
    pub fn get_table_mut(&mut self) -> &'static mut [TableEntry] {
        unsafe { slice::from_raw_parts_mut(self.ptr, ENTRIES_IN_TABLE) }
    }
}

impl Display for VirtualAddressSpace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "VirtualAddressSpace {{ ptr: {:p}, is_user: {} }}",
            self.ptr, self.is_user
        )
    }
}

impl Drop for VirtualAddressSpace {
    fn drop(&mut self) {
        // TODO
    }
}

#[derive(Debug)]
pub struct AddrSpaceLock {
    inner: Mutex<VirtualAddressSpace>,
}

impl AddrSpaceLock {
    pub fn new(data: VirtualAddressSpace) -> Self {
        Self {
            inner: Mutex::new(data),
        }
    }

    #[inline]
    pub fn lock(&self) -> MutexGuard<VirtualAddressSpace> {
        self.inner.lock()
    }

    #[inline]
    pub fn is_user(&self) -> bool {
        unsafe { &*self.inner.data_ptr() }.is_user
    }
}

#[derive(Debug)]
pub enum AddrSpaceSelector<'a> {
    Locked(&'a AddrSpaceLock),
    Unlocked(&'a mut VirtualAddressSpace),
}

impl<'a> AddrSpaceSelector<'a> {
    #[inline]
    pub fn kernel() -> Self {
        Self::Locked(vmm::get_kernel_addr_space())
    }

    pub fn lock(self) -> Guard<'a> {
        match self {
            Self::Locked(lock) => Guard {
                inner: GuardInnerEnum::Owned(lock.lock()),
            },
            Self::Unlocked(guard) => Guard {
                inner: GuardInnerEnum::Ref(guard),
            },
        }
    }
}

pub enum GuardInnerEnum<'a> {
    Owned(MutexGuard<'a, VirtualAddressSpace>),
    Ref(&'a mut VirtualAddressSpace),
}

pub struct Guard<'a> {
    inner: GuardInnerEnum<'a>,
}

impl<'a> Deref for Guard<'a> {
    type Target = VirtualAddressSpace;
    fn deref(&self) -> &Self::Target {
        match self.inner {
            GuardInnerEnum::Owned(ref guard) => guard.deref(),
            GuardInnerEnum::Ref(ref r) => r,
        }
    }
}

impl<'a> DerefMut for Guard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self.inner {
            GuardInnerEnum::Owned(ref mut guard) => guard.deref_mut(),
            GuardInnerEnum::Ref(ref mut r) => r,
        }
    }
}
