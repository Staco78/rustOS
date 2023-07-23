use core::{
    mem,
    sync::atomic::{AtomicUsize, Ordering}, time::Duration,
};

use cortex_a::registers::{CNTFRQ_EL0, CNTPCT_EL0, CNTP_CTL_EL0, CNTP_CVAL_EL0, CNTP_TVAL_EL0};
use log::{info, trace};
use tock_registers::interfaces::{ReadWriteable, Readable, Writeable};

use crate::{cpu::InterruptFrame, interrupts};

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
    interrupts::set_irq_handler(INTERRUPT_ID, self::handler, 0);

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

fn handler(id: u32, frame: *mut InterruptFrame, _: usize) -> *mut InterruptFrame {
    CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::SET);
    let handler_ptr: interrupts::Handler =
        unsafe { mem::transmute(HANDLER.load(Ordering::Relaxed)) };
    handler_ptr(id, frame, 0)
}

// set the timer to fire in ?
pub fn tick_in(duration: Duration) {
    trace!(target: "timer", "Tick in {:?}", duration);
    CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
    let ns = duration.as_nanos();
    let ticks = (ns as f64 / ns_per_tick()) as u64;
    CNTP_TVAL_EL0.set(ticks);
}

pub fn tick_at(time_point: Duration) {
    trace!(target: "timer", "Tick at {:?}", time_point);
    CNTP_CTL_EL0.modify(CNTP_CTL_EL0::IMASK::CLEAR);
    let ns = time_point.as_nanos();
    let ticks = (ns as f64 / ns_per_tick()) as u64;
    CNTP_CVAL_EL0.set(ticks);
}

#[inline]
pub fn uptime() -> Duration {
    let ns = (CNTPCT_EL0.get() as f64 * ns_per_tick()) as u64;
    Duration::from_nanos(ns)
}
