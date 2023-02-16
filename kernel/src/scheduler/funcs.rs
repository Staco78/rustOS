use log::trace;

use crate::{scheduler::SCHEDULER, timer};

use super::{
    process::ProcessRef,
    thread::{ThreadRef, ThreadState},
    Cpu,
};

#[inline]
pub fn current_thread() -> &'static ThreadRef {
    let cpu = Cpu::current();
    cpu.current_thread()
}

#[inline]
#[allow(unused)]
pub fn current_process() -> &'static ProcessRef {
    current_thread().process()
}

pub fn exit(code: isize) -> ! {
    {
        let cpu = Cpu::current();
        let thread = cpu.current_thread();
        debug_assert!(thread.state() == ThreadState::Running);
        thread.atomic_state().store(ThreadState::Exited);

        trace!(target: "scheduler", "Thread {} of process {} exited with code {} on core {}", thread.id(), thread.process().id(), code, cpu.id);

        SCHEDULER.threads_to_destroy.lock().push(thread.clone());
        debug_assert_eq!(
            Cpu::current()
                .irqs_depth
                .load(core::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    yield_now();
    unreachable!()
}

#[inline]
pub fn yield_now() {
    SCHEDULER.yield_now()
}

pub fn sleep(ns: u64) {
    {
        let ns = timer::uptime_ns() + ns;
        let mut threads = SCHEDULER.waiting_threads().write();
        let r = threads.binary_search_by(|e| {
            let time = match e.state() {
                ThreadState::Waiting(time) => time,
                _ => unreachable!(),
            };
            ns.cmp(&time)
        });
        let thread = current_thread().clone();
        thread.atomic_state().store(ThreadState::Waiting(ns));
        match r {
            Ok(i) => threads.insert(i, thread),
            Err(i) => threads.insert(i, thread),
        };
    }

    yield_now();
}
