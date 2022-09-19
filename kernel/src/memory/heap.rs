use core::alloc::GlobalAlloc;

struct Allocator {}

impl Allocator {
    pub const fn new() -> Self {
        Self {}
    }

    pub unsafe fn init() {}
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {}
}
