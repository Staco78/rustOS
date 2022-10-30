use core::{
    mem,
    sync::atomic::{AtomicUsize, Ordering},
};

use cortex_a::registers::{CNTFRQ_EL0, CNTP_CTL_EL0, CNTP_TVAL_EL0};
use tock_registers::interfaces::{Readable, Writeable};

use crate::{cpu::InterruptFrame, interrupts::interrupts};

const INTERRUPT_ID: u32 = 30;
static HANDLER: AtomicUsize = AtomicUsize::new(0);

pub fn init(handler: interrupts::Handler) {
    CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET + CNTP_CTL_EL0::IMASK::CLEAR);
    CNTP_TVAL_EL0.set(frequency());
    interrupts::set_irq_handler(INTERRUPT_ID, self::handler);
    interrupts::chip().enable_interrupt(INTERRUPT_ID);

    let handler_ptr = handler as usize;
    HANDLER.store(handler_ptr, Ordering::Relaxed);
}

#[inline]
// return the frequency of the timer in Hz
pub fn frequency() -> u64 {
    CNTFRQ_EL0.get()
}

#[inline]
fn handler(frame: *mut InterruptFrame) -> *mut InterruptFrame {
    CNTP_TVAL_EL0.set(frequency());
    let handler_ptr: interrupts::Handler = unsafe { mem::transmute(HANDLER.load(Ordering::Relaxed)) };
    handler_ptr(frame)
}
