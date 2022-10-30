use log::trace;

use crate::scheduler::SCHEDULER;

use super::{thread::ThreadState, Cpu};

pub fn exit(code: isize) -> ! {
    let cpu = Cpu::current();
    let thread = cpu.current_thread();
    assert!(thread.state() == ThreadState::Running);
    thread.atomic_state().store(ThreadState::Exited);

    trace!(target: "scheduler", "Thread {} of process {} exited with code {} on core {}", thread.id(), thread.process().id(), code, cpu.id);

    SCHEDULER.threads_to_destroy.lock().push(thread);

    yield_now();
    unreachable!()
}

#[inline]
pub fn yield_now() {
    SCHEDULER.yield_now()
}
