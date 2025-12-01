use core::{
    fmt::Display,
    ops::{Deref, DerefMut},
    slice,
};

use crate::sync::no_irq_locks::{NoIrqMutex, NoIrqMutexGuard};

use super::{
    mmu::TableEntry,
    vmm::{self},
    PageAllocator, PhysicalAddress, ENTRIES_IN_TABLE, PAGE_SIZE, PMM_PAGE_ALLOCATOR,
};

#[derive(Debug)]
pub struct VirtualAddressSpace {
    pub ptr: *mut TableEntry, // the first table
    pub is_low: bool,         // TTBR0 or TTBR1 (before or after hole)
}

impl VirtualAddressSpace {
    pub unsafe fn new(addr: PhysicalAddress, is_low: bool) -> Self {
        debug_assert!(addr.addr() != 0);
        let ptr = addr.to_virt().as_ptr::<TableEntry>();
        Self { ptr, is_low }
    }

    // return None if out of memory
    pub fn create_low() -> Option<Self> {
        let l1 = PMM_PAGE_ALLOCATOR.get().unwrap().alloc(1)?;

        let ptr: *mut u8 = l1.to_virt().as_ptr();
        unsafe { ptr.write_bytes(0, PAGE_SIZE) };

        Some(unsafe { Self::new(l1, true) })
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
            "VirtualAddressSpace {{ ptr: {:p}, is_low: {} }}",
            self.ptr, self.is_low
        )
    }
}

impl Drop for VirtualAddressSpace {
    fn drop(&mut self) {
        // TODO
    }
}

#[derive(Debug)]
pub enum AddrSpaceLock {
    Owned(NoIrqMutex<VirtualAddressSpace>),
    Ref(&'static AddrSpaceLock),
}

impl AddrSpaceLock {
    #[inline]
    pub fn new_owned(data: VirtualAddressSpace) -> Self {
        Self::Owned(NoIrqMutex::new(data))
    }

    #[inline]
    pub fn lock(&self) -> NoIrqMutexGuard<'_, VirtualAddressSpace> {
        match self {
            Self::Owned(lock) => lock.lock(),
            Self::Ref(lock) => lock.lock(),
        }
    }

    #[inline]
    pub fn is_low(&self) -> bool {
        match self {
            Self::Owned(lock) => unsafe { &*lock.data_ptr() }.is_low,
            Self::Ref(lock) => lock.is_low(),
        }
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
    Owned(NoIrqMutexGuard<'a, VirtualAddressSpace>),
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
