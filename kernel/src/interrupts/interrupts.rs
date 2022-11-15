use core::{
    cell::SyncUnsafeCell,
    mem,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::sync::Arc;
use log::trace;

use crate::{acpi::madt::Madt, cpu::InterruptFrame, devices::gic_v2::GenericInterruptController};

pub trait InterruptsChip: Sync + Send {
    fn init(&self);
    fn init_ap(&self);
    fn enable_interrupt(&self, interrupt: u32);
    fn disable_interrupt(&self, interrupt: u32);

    fn get_current_intid(&self) -> u32;
    fn end_of_interrupt(&self, interrupt: u32);

    // send software generated interrupt
    fn send_sgi(&self, destination: CoreSelection, interrupt_id: u8);
}

#[allow(unused)]
pub enum CoreSelection {
    Mask(u8), // Each bit refer to the corresponding CPU (bit 0 for CPU 0). Set the bit to 1 to select.
    Others,
    Me,
}

static CHIP: SyncUnsafeCell<Option<Arc<dyn InterruptsChip>>> = SyncUnsafeCell::new(None);

pub fn init_chip(madt: &Madt) {
    let gic = GenericInterruptController::new(madt);
    gic.init();
    unsafe { *CHIP.get() = Some(Arc::new(gic)) };
}

pub fn chip() -> &'static dyn InterruptsChip {
    let chip = unsafe { &*CHIP.get() };
    chip.as_ref().expect("Interrupt chip not init").as_ref()
}

pub type Handler = fn(*mut InterruptFrame) -> *mut InterruptFrame;
const DEFAULT_HANDLER: AtomicUsize = AtomicUsize::new(0);
static IRQ_HANDLERS: [AtomicUsize; 1020] = [DEFAULT_HANDLER; 1020];

#[no_mangle]
unsafe extern "C" fn interrupt_handler(frame: *mut InterruptFrame) -> *mut InterruptFrame {
    let id = chip().get_current_intid();
    trace!(target: "interrupts", "Receive IRQ {}", id);

    // spurious interrupt
    if id >= 1020 {
        return frame;
    }

    let handler_ptr = IRQ_HANDLERS[id as usize].load(Ordering::Relaxed);
    assert!(handler_ptr != 0);
    let handler: Handler = mem::transmute(handler_ptr);
    let r = handler(frame);
    chip().end_of_interrupt(id);
    r
}

// handler will be run with interrupt disabled
pub fn set_irq_handler(id: u32, handler: Handler) {
    assert!(id < 1020);
    let ptr = handler as usize;
    IRQ_HANDLERS[id as usize].store(ptr, Ordering::Relaxed);
}
