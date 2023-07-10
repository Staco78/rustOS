use core::{
    mem,
    sync::atomic::{AtomicU128, Ordering},
};

use log::trace;
use spin::lock_api::RwLock;

use crate::{cpu::InterruptFrame, interrupts::exceptions::get_exception_state, scheduler::Cpu};

pub mod exceptions;

pub trait InterruptsChip: Sync + Send {
    fn init_ap(&self);
    fn enable_interrupt(&self, interrupt: u32);
    fn disable_interrupt(&self, interrupt: u32);

    fn get_current_intid(&self) -> u32;
    fn end_of_interrupt(&self, interrupt: u32);

    // send software generated interrupt
    fn send_sgi(&self, destination: CoreSelection, interrupt_id: u8);

    fn set_mode(&self, interrupt: u32, mode: InterruptMode);
}

#[derive(Debug)]
pub enum InterruptMode {
    LevelSensitive,
    EdgeTriggered,
}

#[allow(unused)]
pub enum CoreSelection {
    Mask(u8), // Each bit refer to the corresponding CPU (bit 0 for CPU 0). Set the bit to 1 to select.
    Others,
    Me,
}

#[derive(Debug, Clone)]
pub struct MsiVector {
    pub interrupt: u32,
    pub addr: u64,
    pub data: u32,
}

pub trait MsiChip: Sync + Send {
    fn get_free_vector(&self) -> Option<MsiVector>;
}

static CHIP: RwLock<Option<&'static dyn InterruptsChip>> = RwLock::new(None);

pub fn register_chip(chip: &'static dyn InterruptsChip) {
    let mut c = CHIP.write();
    assert!(c.is_none(), "Interrupt chip already registered");
    *c = Some(chip);
}

pub fn chip() -> &'static dyn InterruptsChip {
    let chip = CHIP.read();
    chip.expect("Interrupt chip not registered")
}

static MSI_CHIP: RwLock<Option<&'static dyn MsiChip>> = RwLock::new(None);

pub fn register_msi_chip(chip: &'static dyn MsiChip) {
    let mut c = MSI_CHIP.write();
    assert!(c.is_none(), "MSI chip already registered");
    *c = Some(chip);
}

pub fn msi_chip() -> &'static dyn MsiChip {
    let chip = MSI_CHIP.read();
    chip.expect("MSI chip not registered")
}

pub type Handler = fn(u32, *mut InterruptFrame, usize) -> *mut InterruptFrame;
pub type SimpleHandler = fn(u32, usize);
#[allow(clippy::declare_interior_mutable_const)]
const DEFAULT_HANDLER: AtomicU128 = AtomicU128::new(0);
static IRQ_HANDLERS: [AtomicU128; 1020] = [DEFAULT_HANDLER; 1020];

#[no_mangle]
unsafe extern "C" fn interrupt_handler(frame: *mut InterruptFrame) -> *mut InterruptFrame {
    let irq_depth = Cpu::current().irqs_depth.fetch_add(1, Ordering::Relaxed);
    debug_assert_eq!(irq_depth, 0);

    let id = chip().get_current_intid();

    trace!(target: "interrupts", "Receive IRQ {}", id);

    // spurious interrupt
    if id >= 1020 {
        return frame;
    }

    let data = IRQ_HANDLERS[id as usize].load(Ordering::Relaxed);
    let [handler_ptr, val] = unsafe { mem::transmute::<u128, [usize; 2]>(data) };
    assert!(handler_ptr != 0);
    let handler: Handler = mem::transmute(handler_ptr);
    let before = (
        get_exception_state(),
        Cpu::current().irqs_depth.load(Ordering::Relaxed),
    );
    let r = handler(id, frame, val);
    let after = (
        get_exception_state(),
        Cpu::current().irqs_depth.load(Ordering::Relaxed),
    );
    debug_assert_eq!(before, after);

    chip().end_of_interrupt(id);

    let depth = Cpu::current().irqs_depth.fetch_sub(1, Ordering::Relaxed); // don't call anything that use exceptions depth after that
    debug_assert_eq!(irq_depth, depth - 1);
    debug_assert_eq!(depth, 1);

    r
}

/// Handler will be run with interrupt disabled
pub fn set_irq_handler(id: u32, handler: Handler, val: usize) {
    assert!(id < 1020);
    let ptr = handler as usize;
    let data: u128 = unsafe { mem::transmute([ptr, val]) };
    IRQ_HANDLERS[id as usize].store(data, Ordering::Relaxed);
}

pub fn set_simple_irq_handler(id: u32, handler: SimpleHandler, val: usize) {
    static SIMPLE_HANDLERS: [AtomicU128; 1020] = [DEFAULT_HANDLER; 1020];
    assert!(id < 1020);
    let ptr = handler as usize;
    let data: u128 = unsafe { mem::transmute([ptr, val]) };
    SIMPLE_HANDLERS[id as usize].store(data, Ordering::Relaxed);
    set_irq_handler(id, _handler, 0);

    fn _handler(id: u32, frame: *mut InterruptFrame, _: usize) -> *mut InterruptFrame {
        let data = SIMPLE_HANDLERS[id as usize].load(Ordering::Relaxed);
        let [ptr, val] = unsafe { mem::transmute::<u128, [usize; 2]>(data) };
        assert!(ptr != 0);
        let handler: SimpleHandler = unsafe { mem::transmute(ptr) };
        handler(id, val);
        frame
    }
}
