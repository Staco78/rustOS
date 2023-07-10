use core::{
    fmt::Debug,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

use alloc::boxed::Box;

use crate::memory::{PhysicalAddress, VirtualAddress};

#[repr(transparent)]
pub struct Buffer {
    slice: [MaybeUninit<u8>],
}

impl Buffer {
    #[inline(always)]
    pub const fn from_slice(slice: &[MaybeUninit<u8>]) -> &Self {
        unsafe { &*(slice as *const [MaybeUninit<u8>] as *const Self) }
    }
    #[inline(always)]
    pub fn from_slice_mut(slice: &mut [MaybeUninit<u8>]) -> &mut Self {
        unsafe { &mut *(slice as *mut [MaybeUninit<u8>] as *mut Self) }
    }
    #[inline(always)]
    pub const fn from_slice_ptr(slice: *const [MaybeUninit<u8>]) -> *const Self {
        slice as *const Self
    }
    #[inline(always)]
    pub const fn from_slice_ptr_mut(slice: *mut [MaybeUninit<u8>]) -> *mut Self {
        slice as *mut Self
    }
    #[inline(always)]
    pub const fn from_init_slice(slice: &[u8]) -> &Self {
        unsafe { &*(Self::from_slice_ptr(slice as *const [u8] as _)) }
    }

    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.slice.len()
    }

    pub fn new_boxed(len: usize) -> Box<Self> {
        let slice = Box::new_uninit_slice(len);
        let raw = Box::into_raw(slice);
        let raw = Self::from_slice_ptr_mut(raw);
        unsafe { Box::from_raw(raw) }
    }

    #[inline]
    pub fn read(&self, offset: usize, len: usize) -> &[u8] {
        let slice = &self.slice[offset..offset + len];
        // Safety: this is, in fact, UB but I consider that u8 is always init.
        unsafe { MaybeUninit::slice_assume_init_ref(slice) }
    }

    #[inline]
    pub fn write(&mut self, offset: usize, buff: &[u8]) {
        let slice = &mut self.slice[offset..offset + buff.len()];
        MaybeUninit::write_slice(slice, buff);
    }

    pub fn phys(&self) -> PhysicalAddress {
        let ptr = self.slice.as_ptr();
        let addr = VirtualAddress::from_ptr(ptr);
        addr.to_phys().expect("This should be mapped")
    }

    #[inline(always)]
    pub fn inner(&self) -> &[MaybeUninit<u8>] {
        &self.slice
    }

    #[inline(always)]
    pub fn inner_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut self.slice
    }
}

impl Deref for Buffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.read(0, self.len())
    }
}
impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { MaybeUninit::slice_assume_init_mut(&mut self.slice) }
    }
}

impl Debug for Buffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Buffer")
            .field("size", &self.slice.len())
            .finish()
    }
}
