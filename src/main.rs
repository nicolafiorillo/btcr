/*!
Entry point
*/

// #![warn(
//     missing_docs,
//     clippy::missing_docs_in_private_items,
//     clippy::missing_errors_doc,
//     clippy::missing_panics_doc
// )]

pub mod cli;

use crate::cli::command_line::{print_exit_help, run_command_line};

fn main() {
    env_logger::init();
    println!("Bitcoin_rules! (ver. {})", version());
    println!("A Bitcoin node written in Rust for educational purposes.");
    println!();
    println!("This is a work in progress: please do not use it in production.");

    print_exit_help();
    run_command_line();

    println!("Bye.");
}

fn version() -> &'static str {
    const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");
    VERSION.unwrap_or("unknown")
}
