use core::time::Duration;

use log::trace;

use crate::{scheduler::SCHEDULER, timer};

use super::{
    process::ProcessRef,
    thread::{ThreadId, ThreadRef, ThreadState},
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

#[inline(always)]
pub fn yield_now() {
    SCHEDULER.yield_now()
}

pub fn sleep(duration: Duration) {
    {
        let time_point = timer::uptime() + duration;
        let mut threads = SCHEDULER.waiting_threads().write();
        let r = threads.binary_search_by(|e| {
            let time = match e.state() {
                ThreadState::Waiting(time) => time,
                _ => unreachable!(),
            };
            time_point.cmp(&time)
        });
        let current_thread = current_thread().clone();
        let id = current_thread.id();
        current_thread
            .atomic_state()
            .store(ThreadState::Waiting(time_point));
        match r {
            Ok(i) => threads.insert(i, current_thread),
            Err(i) => threads.insert(i, current_thread),
        };

        trace!(target: "scheduler", "Thread {} goes to sleep for {:?}", id, duration);
    }

    yield_now();
}

/// Get a thread by its id.
///
/// **Warn**: This is O(n) with n as the thread count and may block the scheduler work so use carefully.
pub fn get_thread(id: ThreadId) -> Option<ThreadRef> {
    if current_thread().id() == id {
        return Some(current_thread().clone());
    }
    for cpu in SCHEDULER.cpus() {
        for thread in cpu.threads().lock().iter() {
            if thread.id() == id {
                return Some(thread.clone());
            }
        }
    }

    None
}

/// Set the current state as `Blocked` state and go to sleep.
pub fn block_thread() {
    block_thread_drop(());
}

#[inline]
/// Same as `block_thread` but also drop `val` while owning a lock that prevent others thread from unblocking it.
///
/// May help to prevent race conditions if `val` is a lock guard.
pub fn block_thread_drop<T>(val: T) {
    let current_thread = current_thread();
    current_thread.atomic_state().store(ThreadState::Blocked);

    trace!(target: "scheduler", "Block thread {}", current_thread.id());

    let mut threads = SCHEDULER.blocked_threads.lock();
    threads.push(current_thread.clone());

    drop(val);

    drop(threads);

    yield_now();
}

/// Unblock the thread. Return err if the thread isn't blocked.
pub fn unblock_thread(id: ThreadId) -> Result<(), ()> {
    let mut blocked_threads = SCHEDULER.blocked_threads.lock();
    let (index, _) = blocked_threads
        .iter()
        .enumerate()
        .find(|&(_, t)| t.id() == id)
        .ok_or(())?;

    trace!(target: "scheduler", "Unblock thread {}", id);

    let thread = blocked_threads.swap_remove(index);
    let r = thread.atomic_state().swap(ThreadState::Runnable);
    debug_assert_eq!(r, ThreadState::Blocked);
    SCHEDULER.add_thread(thread);
    Ok(())
}
