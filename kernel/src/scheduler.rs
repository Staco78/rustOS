use core::{
    arch::asm,
    cell::SyncUnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU32, Ordering},
};

use alloc::{collections::VecDeque, vec::Vec};
use cortex_a::{
    asm,
    registers::{DAIF, TPIDR_EL1},
};
use crossbeam_utils::atomic::AtomicCell;
use log::{info, trace};
use static_assertions::const_assert;
use tock_registers::interfaces::{Readable, Writeable};

use crate::{
    cpu::{self, InterruptFrame},
    device_tree,
    interrupts::interrupts::{self, CoreSelection},
    memory::{vmm, AddrSpaceLock},
    scheduler::{
        process::Process,
        thread::{Thread, ThreadEntry},
    },
    timer,
    utils::no_irq_locks::{NoIrqMutex, NoIrqRwLock},
};

use self::{
    process::ProcessRef,
    thread::{ThreadRef, ThreadState},
};

pub mod consts;
mod funcs;
pub mod process;
mod smp;
pub mod sync_ref;
pub mod thread;

pub use funcs::*;
pub use smp::register_cpus;

const TIMESLICE_NS: u64 = 100_000_000; // 100 ms

extern "C" {
    fn exception_exit(frame: *mut InterruptFrame) -> !;
}

static DUMMY_CPU: Cpu = Cpu {
    id: 0,
    is_main_cpu: true,
    current_thread: SyncUnsafeCell::new(None),
    idle_thread: None,
    threads: None,
    irqs_depth: AtomicU32::new(1),
};

pub static SCHEDULER: Scheduler = Scheduler::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerState {
    Initing,
    Started,
}

#[derive(Debug)]
pub struct Scheduler {
    cpus: SyncUnsafeCell<Vec<Cpu>>,
    state: AtomicCell<SchedulerState>,
    kernel_process: SyncUnsafeCell<Option<ProcessRef>>,

    threads_to_destroy: NoIrqMutex<Vec<ThreadRef>>,
    thread_destroyer_of_threads: SyncUnsafeCell<Option<ThreadRef>>,

    waiting_threads: SyncUnsafeCell<MaybeUninit<NoIrqRwLock<VecDeque<ThreadRef>>>>,
}

unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

impl Scheduler {
    const fn new() -> Self {
        Self {
            cpus: SyncUnsafeCell::new(Vec::new()),
            state: AtomicCell::new(SchedulerState::Initing),
            kernel_process: SyncUnsafeCell::new(None),

            threads_to_destroy: NoIrqMutex::new(Vec::new()),
            thread_destroyer_of_threads: SyncUnsafeCell::new(None),

            waiting_threads: SyncUnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    // call this only once after memory init (heap should be available)
    pub fn init(&self) {
        let kernel_process_ = unsafe { &mut *self.kernel_process.get() };

        assert!(self.state.load() == SchedulerState::Initing);
        debug_assert!(kernel_process_.is_none());

        let kernel_addr_space = AddrSpaceLock::Ref(vmm::get_kernel_addr_space());
        let kernel_process = Process::new(kernel_addr_space).into_ref();
        *kernel_process_ = Some(kernel_process);

        let thread_destroyer_of_threads = Thread::new(
            self.get_kernel_process(),
            Self::thread_destroyer_of_threads,
            false,
        )
        .expect("Unable to create the thread destroyer of threads");
        unsafe {
            *self.thread_destroyer_of_threads.get() = Some(thread_destroyer_of_threads);
            *self.waiting_threads.get() = MaybeUninit::new(NoIrqRwLock::new(VecDeque::new()));
        }

        interrupts::set_irq_handler(0, Self::yield_handler);
        timer::init(Self::timer_handler);

        smp::start_cpus();
    }

    #[inline]
    pub fn try_get_kernel_process(&self) -> Option<&ProcessRef> {
        unsafe { &*self.kernel_process.get() }.as_ref()
    }

    #[inline]
    pub fn get_kernel_process(&self) -> &ProcessRef {
        self.try_get_kernel_process()
            .expect("Scheduler not pre_init")
    }

    #[inline]
    fn cpus(&self) -> &[Cpu] {
        // safety: cpus is writed only at initing by only one thread
        unsafe { self.cpus.get().as_ref().unwrap_unchecked() }
    }

    #[inline]
    fn waiting_threads(&self) -> &NoIrqRwLock<VecDeque<ThreadRef>> {
        unsafe { (*self.waiting_threads.get()).assume_init_ref() }
    }

    pub fn register_cpu(&self, id: u32, is_main_cpu: bool) {
        assert!(self.state.load() == SchedulerState::Initing);
        for cpu in self.cpus() {
            assert!(cpu.id != id);
            assert!(!(is_main_cpu && cpu.is_main_cpu));
        }

        let cpu = Cpu::new(id, is_main_cpu);
        unsafe { self.cpus.get().as_mut().unwrap_unchecked() }.push(cpu);
    }

    // call this on each core
    pub fn start(&self, entry: ThreadEntry) -> ! {
        assert!(self.state.load() == SchedulerState::Initing);

        // use a scope here bc rust doesn't drop variables when calling a never return function
        let thread_to_run = {
            let cpu = self
                .cpus()
                .iter()
                .find(|c| c.id == cpu::id())
                .expect("Cpu not registered");
            unsafe { Cpu::set_current(cpu) };

            let idle_thread = Thread::new(self.get_kernel_process(), idle_thread, true).unwrap();
            unsafe {
                let cpu_mut = cpu as *const Cpu as *mut Cpu;
                (*cpu_mut).idle_thread = Some(idle_thread);
            }

            let thread = Thread::new(SCHEDULER.get_kernel_process(), entry, false).unwrap();
            self.add_thread(thread.clone());

            timer::init_core();
            self.state.store(SchedulerState::Started);

            thread.atomic_state().store(ThreadState::Running);
            cpu.set_current_thread(thread.clone());

            if cpu::id() == device_tree::get_boot_cpu_id() {
                self.add_thread(
                    unsafe { &*self.thread_destroyer_of_threads.get() }
                        .as_ref()
                        .unwrap()
                        .clone(),
                );

                info!(target: "scheduler", "Scheduler started");
            }

            self.config_timer(1);
            thread
        };

        unsafe {
            let context = thread_to_run.read().saved_context();
            drop(thread_to_run);
            let r = Cpu::current().irqs_depth.fetch_sub(1, Ordering::Relaxed);
            debug_assert_eq!(r, 1);
            exception_exit(context)
        }
    }

    fn timer_handler(_frame: *mut InterruptFrame) -> *mut InterruptFrame {
        let thread = SCHEDULER.schedule();
        let thread = thread.read();
        thread.saved_context()
    }

    // called by the timer and yield handlers
    // return the thread to run
    fn schedule(&self) -> ThreadRef {
        let cpu = Cpu::current();
        let current_thread = cpu.current_thread();
        let can_rerun = {
            if current_thread.state() == ThreadState::Running {
                current_thread.atomic_state().store(ThreadState::Runnable);
                !current_thread.is_idle_thread()
            } else {
                false
            }
        };

        self.wake_up_waiting_threads();

        let mut threads = cpu.threads().lock();

        if can_rerun {
            threads.push_back(current_thread.clone());
        }

        let next_thread = loop {
            let thread = threads
                .pop_front()
                .unwrap_or_else(|| cpu.idle_thread().clone());

            if thread.state() == ThreadState::Runnable {
                break thread;
            }
        };

        {
            next_thread.atomic_state().store(ThreadState::Running);
            cpu.set_current_thread(next_thread.clone());
        }

        let threads_len = threads.len();
        drop(threads); // unlock threads
        self.config_timer(threads_len);

        trace!(target: "scheduler", "Run thread {} of process {} on CPU {}", next_thread.id(), next_thread.process().id(), cpu.id);

        next_thread
    }

    fn config_timer(&self, runnable_threads_count: usize) {
        let lower_waiting_time =
            self.waiting_threads()
                .read()
                .get(0)
                .and_then(|t| match t.state() {
                    ThreadState::Waiting(time) => Some(time),
                    _ => unreachable!(),
                });

        match (runnable_threads_count == 0, lower_waiting_time) {
            (true, None) => {} // don't set the timer
            (true, Some(ns)) => {
                timer::tick_at_ns(ns);
            }
            (false, None) => timer::tick_in_ns(TIMESLICE_NS),
            (false, Some(ns)) => {
                let uptime = timer::uptime_ns();
                if uptime >= ns {
                    timer::tick_in_ns(TIMESLICE_NS); // FIXME
                    return;
                }
                let remaining_time = ns - uptime;
                if remaining_time < TIMESLICE_NS {
                    timer::tick_at_ns(ns);
                } else {
                    timer::tick_in_ns(TIMESLICE_NS);
                }
            }
        }
    }

    fn wake_up_waiting_threads(&self) {
        let mut runnable_threads = Cpu::current().threads().lock();
        let mut waiting_threads = self.waiting_threads().write();
        let uptime = timer::uptime_ns();
        while let Some(thread) = waiting_threads.front() {
            let wake_up_time = match thread.state() {
                ThreadState::Waiting(time) => time,
                _ => unreachable!(),
            };
            if wake_up_time - 1000 > uptime {
                break;
            }

            let thread = waiting_threads.pop_front().unwrap(); // take it
            thread.atomic_state().store(ThreadState::Runnable);
            runnable_threads.push_back(thread);
        }
    }

    pub(in crate::scheduler) fn add_thread(&self, thread: ThreadRef) {
        assert!(thread.state() == ThreadState::Runnable);
        let cpu = Cpu::current();
        let mut threads = cpu.threads().lock();
        threads.push_back(thread);
    }

    #[inline]
    pub fn yield_now(&self) {
        debug_assert!(
            {
                let spsel: u64;
                let current_el: u64;
                unsafe {
                    asm!(
                        "mrs {}, spsel",
                        "mrs {}, currentEl",
                         out(reg) spsel,
                         out(reg) current_el)
                };
                spsel == 0 && current_el == 4
            },
            "CPU should be in EL1t to yield"
        );
        debug_assert_eq!(
            Cpu::current()
                .irqs_depth
                .load(core::sync::atomic::Ordering::Relaxed),
            0
        );
        debug_assert_eq!(DAIF.get(), 0);
        interrupts::chip().send_sgi(CoreSelection::Me, 0);
    }

    fn yield_handler(_frame: *mut InterruptFrame) -> *mut InterruptFrame {
        debug_assert!(Cpu::current().irqs_depth.load(Ordering::Relaxed) > 0);
        let thread = SCHEDULER.schedule();
        let thread = thread.read();
        let context = thread.saved_context();
        drop(thread);
        context
    }

    fn thread_destroyer_of_threads() -> ! {
        let scheduler = &SCHEDULER;
        loop {
            let mut threads_to_destroy = scheduler.threads_to_destroy.lock();
            for thread in threads_to_destroy.iter() {
                let mut process = thread.process().write();
                let remove_index = process
                    .threads
                    .iter()
                    .enumerate()
                    .find(|e| e.1.id() == thread.id())
                    .unwrap()
                    .0;
                process.threads.swap_remove(remove_index);

                if process.threads.is_empty() {
                    todo!("destroy process");
                }
            }
            threads_to_destroy.clear(); // drop all threads

            drop(threads_to_destroy); // unlock before going to sleep

            sleep(1_000_000_000); // 1 s
        }
    }
}

fn idle_thread() -> ! {
    loop {
        asm::wfe();
    }
}

// each core has a ptr to its own Cpu struct in TPIDR_EL1
#[derive(Debug)]
pub struct Cpu {
    pub id: u32,
    pub is_main_cpu: bool,
    threads: Option<NoIrqMutex<VecDeque<ThreadRef>>>,
    idle_thread: Option<ThreadRef>,
    current_thread: SyncUnsafeCell<Option<ThreadRef>>,
    pub irqs_depth: AtomicU32,
}

const_assert!(AtomicCell::<Option<ThreadRef>>::is_lock_free());

impl Cpu {
    fn new(id: u32, is_main_cpu: bool) -> Self {
        Self {
            id,
            is_main_cpu,
            threads: Some(Default::default()),
            idle_thread: None,
            current_thread: SyncUnsafeCell::new(None),
            irqs_depth: 1.into(),
        }
    }

    #[inline(always)]
    pub fn threads(&self) -> &NoIrqMutex<VecDeque<ThreadRef>> {
        self.threads
            .as_ref()
            .expect("Called threads() on a dummy CPU")
    }

    #[inline(always)]
    pub fn current() -> &'static Cpu {
        let val = TPIDR_EL1.get();
        if val == 0 {
            return &DUMMY_CPU;
        }

        unsafe { (val as *const Cpu).as_ref().unwrap_unchecked() }
    }

    // safety: cpu should live enough
    unsafe fn set_current(cpu: &Cpu) {
        let ptr: *const Cpu = cpu;
        TPIDR_EL1.set(ptr.addr() as u64);
    }

    #[inline]
    fn idle_thread(&self) -> &ThreadRef {
        self.idle_thread.as_ref().expect("No idle thread")
    }

    #[inline]
    fn current_thread(&self) -> &ThreadRef {
        let ptr = self.current_thread.get();
        unsafe { (*ptr).as_ref().expect("No current thread") }
    }

    #[inline]
    fn set_current_thread(&self, thread: ThreadRef) {
        unsafe { *self.current_thread.get() = Some(thread) }
    }
}
