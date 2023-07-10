use core::{
    mem::size_of,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    slice,
};

use crate::error::Error;

use super::{
    vmm::{vmm, MapFlags},
    AddrSpaceSelector, MemoryUsage, PhysicalAddress, VirtualAddress, PAGE_SIZE,
};

#[derive(Debug)]
pub struct Dma<T: ?Sized> {
    phys: PhysicalAddress,
    page_count: usize,
    ptr: NonNull<T>,
}

impl<T: ?Sized> Deref for Dma<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ?Sized> DerefMut for Dma<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T: ?Sized> Drop for Dma<T> {
    fn drop(&mut self) {
        vmm()
            .dealloc_pages(
                VirtualAddress::new(self.ptr.as_ptr() as *const () as usize),
                self.page_count,
                AddrSpaceSelector::kernel(),
            )
            .unwrap();
    }
}

impl<T: ?Sized> Dma<T> {
    #[inline(always)]
    pub fn phys(&self) -> PhysicalAddress {
        self.phys
    }

    #[inline(always)]
    pub fn ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T> Dma<T> {
    pub unsafe fn new() -> Result<Self, Error> {
        let size = size_of::<T>();
        let page_count = size.div_ceil(PAGE_SIZE);
        let vaddr = vmm().alloc_pages(
            page_count,
            MemoryUsage::KernelData,
            MapFlags::new(false, false, 0, 2, false),
            AddrSpaceSelector::kernel(),
        )?;
        let phys = vaddr.to_phys().unwrap();

        Ok(Self {
            phys,
            page_count,
            ptr: unsafe { NonNull::new_unchecked(vaddr.as_ptr()) },
        })
    }
}

impl<T> Dma<[T]> {
    pub unsafe fn new_slice(len: usize) -> Result<Self, Error> {
        let size = size_of::<T>() * len;
        let page_count = size.div_ceil(PAGE_SIZE);
        let vaddr = vmm().alloc_pages(
            page_count,
            MemoryUsage::KernelData,
            MapFlags::new(false, false, 0, 2, false),
            AddrSpaceSelector::kernel(),
        )?;
        let phys = vaddr.to_phys().unwrap();
        let slice = unsafe { slice::from_raw_parts_mut(vaddr.as_ptr::<T>(), len) };

        Ok(Self {
            phys,
            page_count,
            ptr: unsafe { NonNull::new_unchecked(&mut *slice) },
        })
    }
}
