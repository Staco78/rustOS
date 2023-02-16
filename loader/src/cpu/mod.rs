use log::error;

#[panic_handler]
pub fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        if let Some(message) = info.message() {
            error!(
                "Kernel panic in {} at ({}, {}): {}",
                location.file(),
                location.line(),
                location.column(),
                message
            );
        } else {
            error!(
                "Kernel panic in {} at ({}, {})",
                location.file(),
                location.line(),
                location.column(),
            );
        }
    }

    halt();
}

pub fn halt() -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}

#[macro_export]
macro_rules! read_cpu_reg {
    ($r:expr) => {
        unsafe {
            let mut o: u64;
            core::arch::asm!(concat!("mrs {}, ", $r), out(reg) o);
            o
        }
    };
}

#[macro_export]
macro_rules! write_cpu_reg {
    ($r:expr, $v:expr) => {
        unsafe {
            core::arch::asm!(concat!("msr ", $r, ", {}"), in(reg) $v as u64);
        }
    };
}
