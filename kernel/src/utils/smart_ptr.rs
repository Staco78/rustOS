use core::{
    fmt::Debug,
    marker::{PhantomData, Unsize},
    mem::{self, MaybeUninit},
    ops::{CoerceUnsized, Deref, DerefMut},
    ptr::{self, NonNull},
};

use alloc::{boxed::Box, vec::Vec};
use spin::lock_api::Mutex;

#[derive(Debug)]
pub struct SmartPtrInner<T: ?Sized> {
    ref_count: Mutex<usize>,
    data: T,
}

unsafe impl<T: ?Sized + Send> Send for SmartPtrInner<T> {}
unsafe impl<T: ?Sized + Sync> Sync for SmartPtrInner<T> {}

impl<T> SmartPtrInner<T> {
    const fn new(data: T) -> Self {
        Self {
            ref_count: Mutex::new(0),
            data,
        }
    }
}

pub struct SmartPtr<T: ?Sized> {
    ptr: NonNull<SmartPtrInner<T>>,
}

unsafe impl<T: ?Sized + Send> Send for SmartPtr<T> {}
unsafe impl<T: ?Sized + Sync> Sync for SmartPtr<T> {}

impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<SmartPtr<U>> for SmartPtr<T> {}

impl<T> SmartPtr<T> {
    /// This will put `data` on the heap but will never free it.
    /// If this is and all it's clones are dropped, it cause a memory leak.
    pub fn new_boxed(data: T) -> Self {
        let inner = Box::new(SmartPtrInner::new(data));
        let ptr = Box::leak(inner) as *mut _;
        let ptr = unsafe { NonNull::new_unchecked(ptr) };
        Self { ptr }
    }
}

impl<T: ?Sized> Deref for SmartPtr<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &self.ptr.as_ref().data }
    }
}

impl<T: ?Sized> Clone for SmartPtr<T> {
    fn clone(&self) -> Self {
        let inner = unsafe { self.ptr.as_ref() };
        let mut ref_count = inner.ref_count.lock();
        *ref_count += 1;
        Self { ptr: self.ptr }
    }
}

impl<T: ?Sized> Drop for SmartPtr<T> {
    fn drop(&mut self) {
        let inner = unsafe { self.ptr.as_ref() };
        let mut ref_count = inner.ref_count.lock();
        *ref_count -= 1;
        if *ref_count == 0 {
            // Safety: we can drop it because with `ref_count` == 0 it will never be assumed inited.
            unsafe { ptr::drop_in_place(self.ptr.as_ptr()) };
        }
    }
}

impl<T: ?Sized + Debug> Debug for SmartPtr<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let inner = unsafe { self.ptr.as_ref() };
        f.debug_tuple("SmartPtr").field(&&inner.data).finish()
    }
}

pub trait SmartBuff<T> {
    // Safety: don't change the ref_count value.
    unsafe fn data<'a>(&'a self) -> &'a [SmartPtrInner<MaybeUninit<T>>];

    // Safety: don't change the ref_count value.
    unsafe fn data_mut<'a>(&'a mut self) -> &'a mut [SmartPtrInner<MaybeUninit<T>>];

    fn drop(&self) -> bool;

    #[inline(always)]
    fn len(&self) -> usize {
        unsafe { self.data().len() }
    }

    /// Store `value` if a place is found and return it index and a `SmartPtr` over it.
    /// Return `Err(value)` if buff full.
    fn insert(&self, value: T) -> Result<(usize, SmartPtr<T>), T> {
        let data = unsafe { self.data() };
        let r = data.iter().enumerate().find(|(_, d)| {
            let mut c = d.ref_count.lock();
            let value = *c;
            if value == 0 {
                *c = if self.drop() { 1 } else { 2 };
                mem::forget(c);
                true
            } else {
                false
            }
        });

        let (i, inner) = if let Some((i, inner)) = r {
            (i, inner)
        } else {
            return Err(value);
        };

        let inner: &SmartPtrInner<T> = unsafe {
            // Safety: we can get a mutable reference because we checked that there is
            // no other reference anywhere above.
            #[allow(clippy::cast_ref_to_mut)]
            let ptr: &mut _ = &mut *(&inner.data as *const _ as *mut MaybeUninit<T>);
            MaybeUninit::write(ptr, value);

            // Assume init T inside the `SmartPtrInner`.
            // Safety: we just inited it.
            mem::transmute(inner)
        };

        // The value is inited so we can unlock.
        // Safety: `mem::forget` used above.
        unsafe { inner.ref_count.force_unlock() };

        // Safety: it's a ref so the ptr is valid and non-null.
        let ptr = unsafe { NonNull::new_unchecked(inner as *const _ as *mut SmartPtrInner<T>) };
        let ptr = SmartPtr { ptr };
        Ok((i, ptr))
    }

    /// Return a `SmartPtr` over the value at `index` if it exist another `SmartPtr` over it.
    fn get(&self, index: usize) -> Option<SmartPtr<T>> {
        let data = unsafe { self.data() };
        if index >= data.len() {
            return None;
        }
        let inner = &data[index];
        let mut lock = inner.ref_count.lock();
        if *lock == 0 {
            return None;
        }

        *lock += 1;

        // Safety: it's a ref so the ptr is valid and non-null.
        let ptr = unsafe { NonNull::new_unchecked(inner as *const _ as *mut SmartPtrInner<T>) };
        Some(SmartPtr { ptr })
    }

    #[inline]
    fn iter<'a>(&'a self) -> SmartBuffIter<'a, Self, T>
    where
        Self: Sized,
    {
        SmartBuffIter {
            buff: self,
            index: 0,
            _phamtom: PhantomData,
        }
    }

    /// Try to dealloc the memory. Use it instead of dropping it.
    ///
    /// Return Err(Self) if there is at least 1 `SmartPtr` over a value of this buff.
    fn dealloc(mut self) -> Result<(), Self>
    where
        Self: Sized,
    {
        let max_ref_count = if self.drop() { 0 } else { 1 };
        let data = unsafe { self.data_mut() };
        for inner in data.iter() {
            if *inner.ref_count.lock() > max_ref_count {
                return Err(self);
            }
        }

        unsafe { ptr::drop_in_place(data as *mut _) };
        mem::forget(self);
        Ok(())
    }
}

#[derive(Debug)]
pub struct SmartBuffIter<'a, B: SmartBuff<T>, T> {
    buff: &'a B,
    index: usize,
    _phamtom: PhantomData<T>,
}

impl<'a, B: SmartBuff<T>, T> Iterator for SmartBuffIter<'a, B, T> {
    type Item = SmartPtr<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let len = self.buff.len();
        while self.index < len {
            let index = self.index;
            self.index += 1;
            if let Some(item) = self.buff.get(index) {
                return Some(item);
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct SmartPtrBuff<T> {
    data: Box<[SmartPtrInner<MaybeUninit<T>>]>,

    // If false: never drop a value once it was initialized. So keep `ref_count` as 1.
    drop: bool,
}

impl<T> SmartPtrBuff<T> {
    pub fn new(len: usize, drop: bool) -> Self {
        let mut data: Box<[MaybeUninit<SmartPtrInner<MaybeUninit<T>>>]> =
            Box::new_uninit_slice(len);

        for value in data.iter_mut() {
            let init = SmartPtrInner::new(MaybeUninit::<T>::uninit());
            value.write(init);
        }

        // Transmute MaybeUninit<SmartPtrInner<MaybeUninit<T>>> to SmartPtrInner<MaybeUninit<T>>
        //
        // Safety: we just write all of them.
        let data = unsafe { mem::transmute::<_, Box<[SmartPtrInner<MaybeUninit<T>>]>>(data) };

        Self { data, drop }
    }

    pub fn from_iter<I>(iter: I) -> Self
    where
        I: Iterator<Item = T>,
    {
        let vec: Vec<SmartPtrInner<MaybeUninit<T>>> = iter
            .map(|v| SmartPtrInner {
                ref_count: Mutex::new(1),
                data: MaybeUninit::new(v),
            })
            .collect();
        let data = vec.into_boxed_slice();
        Self { data, drop: false }
    }
}

impl<T> SmartBuff<T> for SmartPtrBuff<T> {
    #[inline(always)]
    unsafe fn data<'a>(&'a self) -> &'a [SmartPtrInner<MaybeUninit<T>>] {
        &self.data
    }
    #[inline(always)]
    unsafe fn data_mut<'a>(&'a mut self) -> &'a mut [SmartPtrInner<MaybeUninit<T>>] {
        &mut self.data
    }
    #[inline(always)]
    fn drop(&self) -> bool {
        self.drop
    }
}

impl<T> Drop for SmartPtrBuff<T> {
    fn drop(&mut self) {
        panic!("SmartPtrBuff should be dropped using dealloc method")
    }
}

#[derive(Debug)]
pub struct SmartPtrSizedBuff<T, const N: usize> {
    data: Box<[SmartPtrInner<MaybeUninit<T>>; N]>,

    // If false: never drop a value once it was initialized. So keep `ref_count` as 1.
    drop: bool,
}

impl<T, const N: usize> SmartPtrSizedBuff<T, N> {
    const INIT: SmartPtrInner<MaybeUninit<T>> = SmartPtrInner::new(MaybeUninit::uninit());

    pub fn new(drop: bool) -> Self {
        let data = Box::new([Self::INIT; N]);
        Self { data, drop }
    }
}

impl<T, const N: usize> SmartBuff<T> for SmartPtrSizedBuff<T, N> {
    #[inline(always)]
    unsafe fn data<'a>(&'a self) -> &'a [SmartPtrInner<MaybeUninit<T>>] {
        self.data.deref()
    }
    #[inline(always)]
    unsafe fn data_mut<'a>(&'a mut self) -> &'a mut [SmartPtrInner<MaybeUninit<T>>] {
        self.data.deref_mut()
    }
    #[inline(always)]
    fn drop(&self) -> bool {
        self.drop
    }
}

impl<T, const N: usize> Drop for SmartPtrSizedBuff<T, N> {
    fn drop(&mut self) {
        panic!("SmartPtrSizedBuff should be dropped using dealloc method")
    }
}
