use core::fmt::Write;

use log::Level;
use module::export;

use crate::{cpu, utils::no_irq_locks::NoIrqMutex};

pub struct KernelLogger {
    lock: NoIrqMutex<()>,
}

static LOGGER: KernelLogger = KernelLogger {
    lock: NoIrqMutex::new(()),
};
#[export]
static KERNEL_LOGGER: &'static dyn log::Log = &LOGGER;

static mut OUTPUT: Option<&'static mut dyn Write> = None;

const TARGET_BLACKLIST_TRACE: &[&str] = &[
    "pmm",
    "vmm",
    "kernel_heap",
    "interrupts",
    "scheduler",
    "timer",
    "smp",
];

impl log::Log for KernelLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        if metadata.level() == Level::Trace
            && TARGET_BLACKLIST_TRACE
                .iter()
                .any(|s| *s == metadata.target())
        {
            return false;
        }
        metadata.level() <= log::STATIC_MAX_LEVEL
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let output: &mut dyn Write = unsafe {
                if OUTPUT.is_some() {
                    OUTPUT.as_deref_mut().unwrap()
                } else {
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "qemu_debug")] {
                            let r = &mut QEMU_OUTPUT as &mut dyn Write;
                            write!(r, "(QEMU) ").unwrap();
                            r
                        } else {
                            return;
                        }
                    }
                }
            };

            let lock = self.lock.lock();

            let level = record.level();
            let color = match level {
                Level::Error => "\x1B[91m", // red and bold
                Level::Warn => "\x1B[93m",  // yellow and bold
                Level::Info => "\x1B[97m",  // white
                Level::Debug => "\x1B[37m", // white
                Level::Trace => "\x1B[37m",
            };
            output.write_str(color).unwrap();

            #[cfg(feature = "logger_cpu_id")]
            write!(output, "[CPU {}] ", cpu::id()).unwrap();

            if level != Level::Info {
                write!(output, "[{}] ", level).unwrap();
            }

            let target = record.target();
            // dont't show automatic target
            if !target.contains("::") && level != Level::Info {
                write!(output, "{}: ", target).unwrap();
            }
            writeln!(output, "{}", record.args()).unwrap();
            output.write_str("\x1B[0m").unwrap(); // reset mode and color

            drop(lock);
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::STATIC_MAX_LEVEL);
}

pub fn set_output(output: &'static mut dyn Write) {
    unsafe {
        OUTPUT = Some(output);
    }
}

#[cfg(feature = "qemu_debug")]
static mut QEMU_OUTPUT: crate::devices::pl011_uart::Pl011 = crate::devices::pl011_uart::Pl011::new(
    crate::memory::PhysicalAddress::new(0x9000000).to_virt(),
);
