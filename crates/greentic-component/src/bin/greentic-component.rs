#![cfg_attr(not(feature = "cli"), allow(dead_code))]

use std::process;

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("greentic-component CLI requires the `cli` feature");
    process::exit(1);
}

#[cfg(feature = "cli")]
fn main() {
    if let Err(err) = greentic_component::cli::main() {
        eprintln!("greentic-component: {err:?}");
        process::exit(1);
    }
}
