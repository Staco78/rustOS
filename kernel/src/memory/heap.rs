use core::{alloc::GlobalAlloc, mem::size_of, ptr};

use log::trace;
use spin::Mutex;
use static_assertions::assert_eq_align;

use crate::utils::ByteSize;

use super::{PageAllocator, constants::PAGE_SIZE};

const MIN_PAGE_COUNT: usize = 16; // minimum page count to alloc from page allocator

pub struct Allocator<'a> {
    page_allocator: Option<&'a dyn PageAllocator>,
    head: Mutex<*mut ChunkHeader>,
}

impl<'a> Allocator<'a> {
    pub const fn new() -> Self {
        Self {
            page_allocator: None,
            head: Mutex::new(ptr::null_mut()),
        }
    }

    pub unsafe fn init(&mut self, page_allocator: &'a dyn PageAllocator) {
        self.page_allocator = Some(page_allocator);
    }

    #[inline]
    fn page_allocator(&self) -> &'a dyn PageAllocator {
        self.page_allocator
            .expect("Heap allocator used before init")
    }

    // size is the min size
    unsafe fn alloc_chunk(&self, size: usize) -> *mut ChunkHeader {
        let size = size + size_of::<ChunkHeader>() + size_of::<BlockHeader>();

        let page_count = if size % PAGE_SIZE == 0 {
            size / PAGE_SIZE
        } else {
            size / PAGE_SIZE + 1
        };

        let page_count = page_count.max(MIN_PAGE_COUNT);

        let chunk = self.page_allocator().alloc(page_count) as *mut ChunkHeader;
        if chunk.is_null() {
            return ptr::null_mut();
        }

        let block: *mut BlockHeader = chunk.byte_add(size_of::<ChunkHeader>()).cast();

        let chunk_ref = chunk.as_mut().unwrap();
        let block_ref = block.as_mut().unwrap();

        let size = page_count * PAGE_SIZE - size_of::<ChunkHeader>() - size_of::<BlockHeader>();

        chunk_ref.prev = ptr::null_mut();
        chunk_ref.next = ptr::null_mut();
        chunk_ref.free = size;
        chunk_ref.page_count = page_count;

        block_ref.prev = ptr::null_mut();
        block_ref.next = ptr::null_mut();
        block_ref.size = chunk_ref.free as u32;
        block_ref.allocated_size = 0;
        block_ref.chunk = chunk;

        chunk
    }
}

unsafe impl<'a> GlobalAlloc for Allocator<'a> {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        trace!(target: "kernel_heap", "Alloc {}", ByteSize(layout.size()));
        assert!(
            layout.align() <= size_of::<usize>(),
            "Alignment should not be more than usize"
        );
        assert!(
            layout.size() <= u32::MAX as usize,
            "Cannot alloc more than 4 GB at once"
        );
        assert!(layout.size() > 0);

        let mut head = self.head.lock();

        if head.is_null() {
            *head = self.alloc_chunk(layout.size());
            if head.is_null() {
                return ptr::null_mut(); // chunk alloc failed
            }
        }

        let size = layout.size() as u32;

        let mut current_chunk = *head;
        loop {
            assert!(!current_chunk.is_null());
            let current_chunk_ref = current_chunk.as_mut().unwrap_unchecked(); // safety: the while condition assert the ptr isn't null

            if current_chunk_ref.free >= size as usize {
                let mut current_block = current_chunk_ref.first();
                assert!(!current_block.is_null());
                while !current_block.is_null() {
                    let current_block_ref = current_block.as_mut().unwrap_unchecked(); // safety: the while condition assert the ptr isn't null

                    if current_block_ref.allocated_size == 0 {
                        // hole
                        if current_block_ref.size >= size {
                            current_block_ref.allocated_size = size;
                            current_chunk_ref.free -= size as usize;
                            return current_block_ref.data();
                        } else {
                            // go to the next
                            current_block = current_block_ref.next;
                            continue;
                        }
                    } else {
                        // block in use
                        let usable_size = current_block_ref.size - current_block_ref.allocated_size;
                        let usable_size = if usable_size >= size_of::<BlockHeader>() as u32 {
                            usable_size - size_of::<BlockHeader>() as u32
                        } else {
                            0
                        };

                        if usable_size >= size {
                            let new_block: *mut BlockHeader = current_block_ref
                                .data()
                                .byte_add(current_block_ref.allocated_size as usize)
                                .cast();
                            let new_block_ref = new_block.as_mut().unwrap();
                            let next = current_block_ref.next;
                            current_block_ref.next = new_block;
                            current_block_ref.size = current_block_ref.allocated_size;
                            new_block_ref.next = next;
                            new_block_ref.prev = current_block;
                            new_block_ref.size = usable_size;
                            new_block_ref.allocated_size = size;
                            new_block_ref.chunk = current_chunk;

                            current_chunk_ref.free -= size as usize + size_of::<BlockHeader>();
                            if !next.is_null() {
                                (*next).prev = new_block;
                            }
                            return new_block_ref.data();
                        } else {
                            // go to the next
                            current_block = current_block_ref.next;
                            continue;
                        }
                    }
                }
            }

            if !current_chunk_ref.next.is_null() {
                current_chunk = current_chunk_ref.next;
                continue;
            }

            // no next chunk so alloc another

            current_chunk_ref.next = self.alloc_chunk(size as usize);
            if current_chunk_ref.next.is_null() {
                // alloc failed
                return ptr::null_mut();
            }
            (*current_chunk_ref.next).prev = current_chunk;
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        trace!(target: "kernel_heap", "Dealloc {} at {:p}", ByteSize(layout.size()), ptr);
        assert!(!ptr.is_null(), "Dealloc with null ptr");
        let mut head = self.head.lock(); // lock

        let block: *mut BlockHeader = ptr.sub(size_of::<BlockHeader>()).cast();
        let mut block_ref = block.as_mut().unwrap();
        assert!(block_ref.allocated_size == layout.size() as u32);

        let chunk = block_ref.chunk;
        let chunk_ref = chunk.as_mut().unwrap();

        block_ref.allocated_size = 0;
        chunk_ref.free += layout.size();

        if !block_ref.prev.is_null() {
            let prev = block_ref.prev;
            let prev_ref = prev.as_mut().unwrap();
            prev_ref.size += block_ref.size + size_of::<BlockHeader>() as u32;
            chunk_ref.free += size_of::<BlockHeader>();
            assert!(prev_ref.next == block);
            prev_ref.next = block_ref.next;

            if !block_ref.next.is_null() {
                let next = block_ref.next;
                let next_ref = next.as_mut().unwrap();
                next_ref.prev = prev;
            }

            if prev_ref.allocated_size == 0 {
                // if the prev block is free set it at "current block" so we can destroy the chunk if it's the last
                block_ref = prev_ref;
            }
        }

        // if it's the last block in the chunk free it
        if block_ref.prev.is_null() && block_ref.next.is_null() {
            if !chunk_ref.prev.is_null() {
                (*chunk_ref.prev).next = chunk_ref.next;
            }
            if !chunk_ref.next.is_null() {
                (*chunk_ref.next).prev = chunk_ref.prev;
            }

            if *head == chunk {
                *head = ptr::null_mut();
            }

            self.page_allocator()
                .dealloc(chunk.addr(), chunk_ref.page_count as usize);
        }
    }
}

// found in start of every chunk
#[derive(Debug)]
struct ChunkHeader {
    prev: *mut ChunkHeader,
    next: *mut ChunkHeader,
    page_count: usize,
    free: usize, // size of free memory
}

assert_eq_align!(ChunkHeader, usize);

impl ChunkHeader {
    #[inline]
    fn first(&mut self) -> *mut BlockHeader {
        let self_ptr: *mut ChunkHeader = self;
        unsafe { self_ptr.byte_add(size_of::<Self>()).cast() }
    }
}

// found in start of every allocated block
#[derive(Debug)]
struct BlockHeader {
    chunk: *mut ChunkHeader,
    prev: *mut BlockHeader,
    next: *mut BlockHeader,
    size: u32,           // size of usable memory
    allocated_size: u32, // used size, 0 = hole
}

assert_eq_align!(BlockHeader, usize);

impl BlockHeader {
    #[inline]
    fn data(&mut self) -> *mut u8 {
        let self_ptr: *mut BlockHeader = self;
        unsafe { self_ptr.byte_add(size_of::<Self>()).cast() }
    }
}
