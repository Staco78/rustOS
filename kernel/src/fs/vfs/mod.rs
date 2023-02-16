mod funcs;
pub mod node;

pub use funcs::*;

#[derive(Debug)]
pub enum OpenError {
    NotFound
}

#[derive(Debug)]
pub enum ReadError {}

#[derive(Debug)]
pub enum WriteError {
    ReadOnly,
}
