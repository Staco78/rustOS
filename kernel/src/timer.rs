use core::{
    mem,
    sync::atomic::{AtomicUsize, Ordering},
};

use cortex_a::registers::{CNTFRQ_EL0, CNTPCT_EL0, CNTP_CTL_EL0, CNTP_CVAL_EL0, CNTP_TVAL_EL0};
use log::{info, trace};
use tock_registers::interfaces::{ReadWriteable, Readable, Writeable};

use crate::{cpu::InterruptFrame, interrupts::interrupts};

const INTERRUPT_ID: u32 = 30;
static HANDLER: AtomicUsize = AtomicUsize::new(0);
static mut NS_PER_TICK: f64 = 0.;

#[inline]
fn ns_per_tick() -> f64 {
    unsafe {
        debug_assert!(NS_PER_TICK != 0.);
        NS_PER_TICK
    }
}

// run once
pub fn init(handler: interrupts::Handler) {
    info!(target: "timer", "Timer initialized");
    unsafe { NS_PER_TICK = 1_000_000_000. / frequency() as f64 };
    interrupts::set_irq_handler(INTERRUPT_ID, self::handler);

    let handler_ptr = handler as usize;
    HANDLER.store(handler_ptr, Ordering::Relaxed);
}

// run once per core
pub fn init_core() {
    interrupts::chip().enable_interrupt(INTERRUPT_ID);
    CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET + CNTP_CTL_EL0::IMASK::CLEAR);
}

#[inline]
// return the frequency of the timer in Hz
pub fn frequency() -> u64 {
    CNTFRQ_EL0.get()
}

fn handler(frame: *mut InterruptFrame) -> *mut InterruptFrame {
    CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::SET);
    let handler_ptr: interrupts::Handler =
        unsafe { mem::transmute(HANDLER.load(Ordering::Relaxed)) };
    handler_ptr(frame)
}

// set the timer to fire in ? ns
pub fn tick_in_ns(ns: u64) {
    trace!(target: "timer", "Tick in {ns} ns");
    CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
    let ticks = (ns as f64 / ns_per_tick()) as u64;
    CNTP_TVAL_EL0.set(ticks);
}

pub fn tick_at_ns(ns: u64) {
    trace!(target: "timer", "Tick at {ns} ns");
    // assert!(ns > uptime_ns());
    CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
    let ticks = (ns as f64 / ns_per_tick()) as u64;
    CNTP_CVAL_EL0.set(ticks);
}

#[inline]
pub fn uptime_ns() -> u64 {
    (CNTPCT_EL0.get() as f64 * ns_per_tick()) as u64
}
