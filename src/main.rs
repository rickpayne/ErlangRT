//! `ErlangRT` is an alternative Erlang BEAM Runtime written in Rust
//!

#![feature(const_fn)]
//#![feature(alloc)] // for rawvec
#![feature(const_size_of)]

// Use from command line instead: `cargo build --features "clippy"` or `make clippy`
//#![cfg_attr(feature="clippy", feature(plugin))]
//#![cfg_attr(feature="clippy", plugin(clippy))]

extern crate bit_field;
extern crate bytes;
extern crate compress;
extern crate num;

#[macro_use]
extern crate lazy_static;

extern crate rt_defs;
extern crate rt_util;

mod beam;
mod bif;
mod emulator;
mod fail;
mod term;

use emulator::atom;
use emulator::scheduler::Prio;
use emulator::mfa::MFArgs;
use emulator::vm::VM;
use term::lterm::*;
//use term::lterm::list_term;


/// Entry point for the command-line interface
fn main() {
  if cfg!(feature = "r19") {
    println!("Erlang Runtime (compat OTP 19)");
  }
  if cfg!(feature = "r20") {
    println!("Erlang Runtime (compat OTP 20)");
  }

  let mut beam = VM::new();

  let mfa = MFArgs::new(
    atom::from_str("test2"),
    atom::from_str("test"),
    Vec::new()
  );
  let _rootp = beam.create_process(
    aspect_list::nil(),
    &mfa,
    Prio::Normal
  ).unwrap();

  println!("Process created. Entering main loop...");
  while beam.tick() {}
}
