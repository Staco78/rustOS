use core::{arch::asm, cell::SyncUnsafeCell};

use alloc::{collections::VecDeque, vec::Vec};
use cortex_a::{asm, registers::TPIDR_EL1};
use crossbeam_utils::atomic::AtomicCell;
use log::{trace, info};
use spin::lock_api::Mutex;
use static_assertions::const_assert;
use tock_registers::interfaces::{Readable, Writeable};

use crate::{
    cpu::InterruptFrame,
    interrupts::interrupts::{self, CoreSelection},
    memory::vmm,
    scheduler::{
        process::Process,
        thread::{Thread, ThreadEntry},
    },
    timer,
};

use self::{
    process::ProcessRef,
    thread::{ThreadRef, ThreadState},
};

pub mod consts;
mod funcs;
pub mod process;
pub mod sync_ref;
pub mod thread;

pub use funcs::*;

extern "C" {
    fn exception_exit(frame: *mut InterruptFrame) -> !;
}

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

    threads_to_destroy: Mutex<Vec<ThreadRef>>,
    thread_destroyer_of_threads: SyncUnsafeCell<Option<ThreadRef>>,
}

unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

impl Scheduler {
    const fn new() -> Self {
        Self {
            cpus: SyncUnsafeCell::new(Vec::new()),
            state: AtomicCell::new(SchedulerState::Initing),
            kernel_process: SyncUnsafeCell::new(None),

            threads_to_destroy: Mutex::new(Vec::new()),
            thread_destroyer_of_threads: SyncUnsafeCell::new(None),
        }
    }

    // call this after memory init (heap should be available)
    pub fn init(&self) {
        let kernel_process_ = unsafe { &mut *self.kernel_process.get() };

        assert!(self.state.load() == SchedulerState::Initing);
        assert!(kernel_process_.is_none());

        let kernel_addr_space = vmm::create_current_kernel_addr_space();
        let kernel_process = Process::new(kernel_addr_space).into_ref();
        *kernel_process_ = Some(kernel_process);

        let thread_destroyer_of_threads = Thread::new(
            self.get_kernel_process(),
            Self::thread_destroyer_of_threads,
            false,
        )
        .expect("Unable to create the thread destroyer of threads");
        unsafe { *self.thread_destroyer_of_threads.get() = Some(thread_destroyer_of_threads) };
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

    pub fn register_cpu(&self, id: u8, is_main_cpu: bool) {
        assert!(self.state.load() == SchedulerState::Initing);
        for cpu in self.cpus() {
            assert!(cpu.id != id);
            assert!(!(is_main_cpu && cpu.is_main_cpu));
        }

        let cpu = Cpu::new(id, is_main_cpu);
        unsafe { self.cpus.get().as_mut().unwrap_unchecked() }.push(cpu);
    }

    pub fn start(&self, cpu_id: u8, entry: ThreadEntry) -> ! {
        assert!(self.state.load() == SchedulerState::Initing);
        
        // use a scope here bc rust doesn't drop variables when calling a never return function
        let thread = {
            let cpu = self
                .cpus()
                .iter()
                .find(|c| c.id == cpu_id)
                .expect("Cpu not registered");
                unsafe {
                    Cpu::set_current(cpu);
            }

            let idle_thread = Thread::new(self.get_kernel_process(), idle_thread, true).unwrap();
            unsafe {
                let cpu_mut = cpu as *const Cpu as *mut Cpu;
                (*cpu_mut).idle_thread = Some(idle_thread);
            }

            let thread = Thread::new(SCHEDULER.get_kernel_process(), entry, false).unwrap();

            interrupts::set_irq_handler(0, Self::yield_handler);
            
            cpu.set_current_thread(thread.clone());
            self.add_thread(thread.clone());
            self.add_thread(
                unsafe { &*self.thread_destroyer_of_threads.get() }
                    .as_ref()
                    .unwrap()
                    .clone(),
            );

            timer::init(Self::timer_handler);
            self.state.store(SchedulerState::Started);
            
            thread
        };
        
        info!(target: "scheduler", "Scheduler started");
        
        unsafe {
            thread.atomic_state().store(ThreadState::Running);
            let context = thread.read().saved_context();
            drop(thread);
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
            if current_thread.state() == ThreadState::Exited {
                false
            } else {
                assert!(current_thread.state() == ThreadState::Running);
                current_thread.atomic_state().store(ThreadState::Paused);
                !current_thread.is_idle_thread()
            }
        };

        let mut threads = cpu.threads.lock();

        if can_rerun {
            threads.push_back(current_thread);
        }

        let next_thread = loop {
            let thread = threads
                .pop_front()
                .unwrap_or_else(|| cpu.idle_thread().clone());

            if thread.state() != ThreadState::Exited {
                break thread;
            }
        };

        {
            assert!(next_thread.state() == ThreadState::Paused);
            next_thread.atomic_state().store(ThreadState::Running);
            cpu.set_current_thread(next_thread.clone());
        }

        trace!(target: "scheduler", "Run thread {} of process {} on CPU {}", next_thread.id(), next_thread.process().id(), cpu.id);

        next_thread
    }

    pub(in crate::scheduler) fn add_thread(&self, thread: ThreadRef) {
        assert!(thread.state() == ThreadState::Paused);
        let cpu = Cpu::current();
        cpu.threads.lock().push_back(thread);
    }

    #[inline]
    pub fn yield_now(&self) {
        assert!(
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
        interrupts::chip().send_sgi(CoreSelection::Me, 0);
    }

    fn yield_handler(_frame: *mut InterruptFrame) -> *mut InterruptFrame {
        let thread = SCHEDULER.schedule();
        let thread = thread.read();
        thread.saved_context()
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

                if process.threads.len() == 0 {
                    todo!("destroy process");
                }
            }
            threads_to_destroy.clear(); // this will drop all threads

            loop {}
            // yield_now();
        }
    }
}

fn idle_thread() -> ! {
    loop {
        asm::wfe();
    }
}

// each core has a ptr to its own Cpu struct in TPIDR_EL1
pub struct Cpu {
    id: u8,
    is_main_cpu: bool,
    threads: Mutex<VecDeque<ThreadRef>>,
    idle_thread: Option<ThreadRef>,
    current_thread: AtomicCell<Option<ThreadRef>>,
}

const_assert!(AtomicCell::<Option<ThreadRef>>::is_lock_free());

impl Cpu {
    fn new(id: u8, is_main_cpu: bool) -> Self {
        Self {
            id,
            is_main_cpu,
            threads: Default::default(),
            idle_thread: None,
            current_thread: AtomicCell::new(None),
        }
    }

    #[inline]
    pub fn current() -> &'static Cpu {
        let val = TPIDR_EL1.get();
        debug_assert!(val != 0, "Cpu not inited");

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
    fn current_thread(&self) -> ThreadRef {
        let ptr = self.current_thread.as_ptr();
        unsafe { Clone::clone(&(*ptr).as_ref().expect("No current thread")) }
    }

    #[inline]
    fn set_current_thread(&self, thread: ThreadRef) {
        self.current_thread.store(Some(thread))
    }
}
