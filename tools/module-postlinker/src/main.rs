use std::env;

use crate::logic::make_obj;

mod got;
mod logic;
mod plt;

fn main() {
    let args: Vec<_> = {
        let mut args = env::args();
        args.next();
        args.collect()
    };

    let params: Vec<_> = args.iter().filter(|e| !e.starts_with("-")).collect();
    let options: Vec<_> = args.iter().filter(|e| e.starts_with("-")).collect();

    for option in options {
        match option.as_str() {
            _ => panic!("Invalid option {}", option),
        }
    }

    assert!(params.len() == 2, "Invalid params len");

    make_obj(params[0], params[1]).unwrap();
}
