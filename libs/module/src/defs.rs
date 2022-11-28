use log::Log;

#[allow(unused)]
extern "Rust" {
    pub fn get_logger() -> &'static dyn Log;
    pub fn panic(info: &core::panic::PanicInfo) -> !;
}
