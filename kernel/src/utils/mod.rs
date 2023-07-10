pub mod buffer;
pub mod byte_size;
pub mod sizes;
pub mod smart_ptr;
pub mod sync_once_cell;

// Stolen from the std and replace eprintln! with debug!
#[macro_export]
macro_rules! dbg {
    () => {
        $log::debug!("[{}:{}]", file!(), line!())
    };
    ($val:expr $(,)?) => {
        log::debug!("[{}:{}] {} = {:#?}",
            file!(), line!(), stringify!($val), &$val)
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}
