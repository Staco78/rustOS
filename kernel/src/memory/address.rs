use core::{
    arch::asm,
    fmt::{Debug, Display},
    marker::PhantomData,
    ops::{Add, AddAssign, Div, Mul, Range, Rem, Sub, SubAssign},
};

use crate::memory::PHYSICAL_LINEAR_MAPPING_RANGE;

use super::{INVALID_VIRT_ADDRESS_RANGE, LOW_ADDR_SPACE_RANGE};

pub trait MemoryKind {
    const INVALID_RANGE: Option<Range<usize>>;
}

pub struct Physical;
impl MemoryKind for Physical {
    const INVALID_RANGE: Option<Range<usize>> = None;
}

pub struct Virtual;
impl MemoryKind for Virtual {
    const INVALID_RANGE: Option<Range<usize>> = Some(INVALID_VIRT_ADDRESS_RANGE);
}

#[repr(transparent)]
pub struct Address<K: MemoryKind>(usize, PhantomData<K>);

/// Represent a virtual memory address. Has the same memory layout as `usize`.
///
/// The valid range is the 512 first GB and the last 256 TB of the address space.
/// In other words, from `0` to `0x8000000000` and from `0xFFFF000000000000` to `0xFFFFFFFFFFFFFFFF`.
pub type VirtualAddress = Address<Virtual>;

/// Represent a physical memory address. Has the same memory layout as `usize`.
pub type PhysicalAddress = Address<Physical>;

impl<K: MemoryKind> Address<K> {
    #[inline]
    /// Create a new `Address`.
    ///
    /// Panic if `value` is outside the valid range.
    pub const fn new(value: usize) -> Self {
        if let Some(invalid_range) = K::INVALID_RANGE {
            if value > invalid_range.start && value < invalid_range.end {
                panic!("Address out of valid range");
            }
        }
        Self(value, PhantomData)
    }

    #[inline]
    /// Get the underlying address.
    pub const fn addr(self) -> usize {
        self.0
    }

    #[inline]
    pub fn is_aligned_to(self, alignment: usize) -> bool {
        self.0 % alignment == 0
    }

    #[inline]
    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

impl PhysicalAddress {
    #[inline]
    /// Transform the `self` `PhysicalAddress` to a `VirtualAddress` by adding a constant.
    ///
    /// `self` must be in the first 512 GB.
    pub const fn to_virt(self) -> VirtualAddress {
        assert!(self >= LOW_ADDR_SPACE_RANGE.start && self <= LOW_ADDR_SPACE_RANGE.end);
        let addr = VirtualAddress::new(self.addr() + PHYSICAL_LINEAR_MAPPING_RANGE.start.addr());
        debug_assert!(
            addr >= PHYSICAL_LINEAR_MAPPING_RANGE.start
                && addr <= PHYSICAL_LINEAR_MAPPING_RANGE.end
        );
        addr
    }
}

impl VirtualAddress {
    #[inline]
    pub fn as_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    #[inline]
    /// Transform the `self` `VirtualAddress` to a `PhysicalAddress` by using the `AT` asm instruction.
    ///
    /// Return `None` if `self` isn't mapped (if the translation fails).
    pub fn to_phys(self) -> Option<PhysicalAddress> {
        let par = unsafe {
            asm!("AT S1E1R, {}", in(reg) self.addr());
            let mut out: usize;
            asm!("mrs {}, PAR_EL1", out(reg) out);
            out
        };
        if (par & 1) == 1 {
            None
        } else {
            let v = (par & 0xFFFFFFFF000) | (self.addr() & 0xFFF);
            Some(PhysicalAddress::new(v))
        }
    }
}

impl<K: MemoryKind> Clone for Address<K> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}
impl<K: MemoryKind> Copy for Address<K> {}
impl<K: MemoryKind> Add for Address<K> {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.0 + rhs.0)
    }
}
impl<K: MemoryKind> Add<usize> for Address<K> {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.0 + rhs)
    }
}
impl<K: MemoryKind> AddAssign<usize> for Address<K> {
    #[inline(always)]
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}
impl<K: MemoryKind> Sub for Address<K> {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.0 - rhs.0)
    }
}
impl<K: MemoryKind> Sub<usize> for Address<K> {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.0 - rhs)
    }
}
impl<K: MemoryKind> SubAssign<usize> for Address<K> {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: usize) {
        self.0 -= rhs;
    }
}

impl<K: MemoryKind> Mul<usize> for Address<K> {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: usize) -> Self::Output {
        Self::new(self.0 * rhs)
    }
}
impl<K: MemoryKind> Div<usize> for Address<K> {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: usize) -> Self::Output {
        Self::new(self.0 / rhs)
    }
}
impl<K: MemoryKind> Rem<usize> for Address<K> {
    type Output = Self;
    #[inline(always)]
    fn rem(self, rhs: usize) -> Self::Output {
        Self::new(self.0 % rhs)
    }
}

impl Debug for VirtualAddress {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VirtualAddress({:p})", self.0 as *const ())
    }
}

impl Debug for PhysicalAddress {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PhysicalAddress({:p})", self.0 as *const ())
    }
}

impl<K: MemoryKind> Display for Address<K> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:p}", self.0 as *const ())
    }
}

impl<K: MemoryKind, K2: MemoryKind> PartialEq<Address<K>> for Address<K2> {
    #[inline(always)]
    fn eq(&self, other: &Address<K>) -> bool {
        self.0.eq(&other.0)
    }
}
impl<K: MemoryKind, K2: MemoryKind> const PartialOrd<Address<K>> for Address<K2> {
    #[inline(always)]
    fn partial_cmp(&self, other: &Address<K>) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}
impl<K: MemoryKind> PartialEq<usize> for Address<K> {
    #[inline(always)]
    fn eq(&self, other: &usize) -> bool {
        self.0.eq(other)
    }
}
impl<K: MemoryKind> PartialOrd<usize> for Address<K> {
    #[inline(always)]
    fn partial_cmp(&self, other: &usize) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}
impl<K: MemoryKind> PartialEq<Address<K>> for usize {
    #[inline(always)]
    fn eq(&self, other: &Address<K>) -> bool {
        self.eq(&other.0)
    }
}
impl<K: MemoryKind> PartialOrd<Address<K>> for usize {
    #[inline(always)]
    fn partial_cmp(&self, other: &Address<K>) -> Option<core::cmp::Ordering> {
        self.partial_cmp(&other.0)
    }
}
