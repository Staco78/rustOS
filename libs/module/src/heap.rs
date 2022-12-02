use core::alloc::GlobalAlloc;
use super::defs::KERNEL_ALLOCATOR;

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

struct Allocator;

unsafe impl GlobalAlloc for Allocator {
    #[inline(always)]
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        KERNEL_ALLOCATOR.alloc(layout)
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        KERNEL_ALLOCATOR.dealloc(ptr, layout)
    }
}
