use core::{
    fmt::Debug,
    mem::size_of,
    sync::atomic::{AtomicUsize, Ordering},
};

use crossbeam_utils::atomic::AtomicCell;
use log::trace;

use crate::{
    cpu::InterruptFrame,
    error::Error,
    memory::{
        vmm::{vmm, MapFlags, MapOptions, MapSize, MemoryUsage},
        AddrSpaceSelector, PhysicalAddress, VirtualAddress, PAGE_SHIFT, PAGE_SIZE,
    },
};

use super::{
    consts::{KERNEL_STACK_PAGE_COUNT, USER_STACK_PAGE_COUNT},
    process::ProcessRef,
    sync_ref::SyncRef,
    Cpu, SCHEDULER,
};

pub type ThreadId = usize;
pub type ThreadRef = SyncRef<Thread>;

pub type ThreadEntry = fn() -> !;

static THREAD_ID: AtomicUsize = AtomicUsize::new(0);

#[inline]
fn get_next_id() -> ThreadId {
    THREAD_ID.fetch_add(1, Ordering::Relaxed) as ThreadId
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Runnable,
    Running,
    Exited,

    /// Store the time in ns from uptime where we will wake up the thread.
    Waiting(u64),

    Blocked,
}

pub struct Thread {
    process: ProcessRef,
    id: ThreadId,
    state: AtomicCell<ThreadState>,

    user_stack_base: VirtualAddress,
    kernel_stack_base: VirtualAddress,
    kernel_stack: VirtualAddress, // also a *mut InterruptFrame

    is_idle_thread: bool,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    /// Create a new thread:
    /// Allocate an user and a kernel stack and
    /// add itself in the thread list of its parent process.
    pub fn new(
        process: &ProcessRef,
        entry: ThreadEntry,
        is_idle_thread: bool,
    ) -> Result<ThreadRef, Error> {
        let id = get_next_id();
        let mut process_lock = process.write();
        let process_id = process_lock.id();

        let addr_space = &mut process_lock.addr_space;

        trace!(target: "scheduler",
            "Create {} thread {} of process {} with entry {:p}",
            if addr_space.is_low() { "user" } else { "kernel" },
            id,
            process_id,
            entry as *const ()
        );

        let user_stack_base = {
            let usage = if addr_space.is_low() {
                MemoryUsage::UserData
            } else {
                MemoryUsage::KernelHeap
            };
            let r = vmm().alloc_pages(
                USER_STACK_PAGE_COUNT + 1,
                usage,
                MapFlags::default(),
                AddrSpaceSelector::Locked(addr_space),
            )?;
            vmm().map_page(
                r,
                PhysicalAddress::new(usize::MAX & !((1 << PAGE_SHIFT) - 1)),
                MapOptions::new(MapSize::Size4KB, MapFlags::new(false, false, 0b11, 1, true)),
                AddrSpaceSelector::Locked(addr_space),
            )?;
            r + PAGE_SIZE
        };

        let kernel_stack_base = vmm().alloc_pages(
            KERNEL_STACK_PAGE_COUNT,
            MemoryUsage::KernelHeap,
            MapFlags::default(),
            AddrSpaceSelector::kernel(),
        )?;
        let kernel_stack =
            kernel_stack_base + KERNEL_STACK_PAGE_COUNT * PAGE_SIZE - size_of::<InterruptFrame>();

        let regs = unsafe {
            (kernel_stack.as_ptr::<InterruptFrame>())
                .as_mut()
                .unwrap_unchecked()
        };

        regs.sp = (user_stack_base + USER_STACK_PAGE_COUNT * PAGE_SIZE).addr();
        regs.pc = entry as usize;
        regs.pstate = 4; // interrupts enabled, EL1t

        let thread = Self {
            process: process.clone(),
            id,
            state: AtomicCell::new(ThreadState::Runnable),
            user_stack_base,
            kernel_stack_base,
            kernel_stack,

            is_idle_thread,
        };

        let thread_ref = ThreadRef::new(thread);
        process_lock.add_thread(thread_ref.clone());

        Ok(thread_ref)
    }

    #[inline]
    pub fn saved_context(&self) -> *mut InterruptFrame {
        debug_assert_ne!(self.kernel_stack, 0);
        self.kernel_stack.as_ptr()
    }
}

impl ThreadRef {
    #[inline]
    pub fn id(&self) -> ThreadId {
        let ptr = self.data_ptr();
        unsafe { (*ptr).id }
    }

    #[inline]
    pub fn state(&self) -> ThreadState {
        let atomic_state = self.atomic_state();
        atomic_state.load()
    }

    #[inline]
    pub fn atomic_state(&self) -> &AtomicCell<ThreadState> {
        let ptr = self.data_ptr();
        unsafe { &(*ptr).state }
    }

    #[inline]
    pub fn is_idle_thread(&self) -> bool {
        let ptr = self.data_ptr();
        unsafe { (*ptr).is_idle_thread }
    }

    #[inline]
    pub fn process(&self) -> &ProcessRef {
        let ptr = self.data_ptr();
        unsafe { &(*ptr).process }
    }

    #[inline]
    #[allow(unused)]
    pub fn start(self) {
        SCHEDULER.add_thread(self);
        SCHEDULER.config_timer(Cpu::current().threads().lock().len());
    }
}

impl PartialEq for ThreadRef {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.id().eq(&other.id())
    }
}
impl Eq for ThreadRef {}

impl PartialEq for Thread {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}
impl Eq for Thread {}

impl Drop for Thread {
    fn drop(&mut self) {
        vmm()
            .dealloc_pages(
                self.user_stack_base,
                USER_STACK_PAGE_COUNT,
                AddrSpaceSelector::Locked(self.process.get_addr_space()),
            )
            .unwrap();
        vmm()
            .dealloc_pages(
                self.kernel_stack_base,
                KERNEL_STACK_PAGE_COUNT,
                AddrSpaceSelector::kernel(),
            )
            .unwrap();
    }
}

impl Debug for Thread {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Thread")
            .field("process", &self.process.id())
            .field("id", &self.id)
            .field("state", &self.state)
            .field("user_stack_base", &self.user_stack_base)
            .field("kernel_stack_base", &self.kernel_stack_base)
            .field("kernel_stack", &self.kernel_stack)
            .field("is_idle_thread", &self.is_idle_thread)
            .finish()
    }
}
