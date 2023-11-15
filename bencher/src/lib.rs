#![cfg_attr(not(feature = "std"), no_std)]

#[doc(hidden)]
pub use frame_support;
#[doc(hidden)]
pub use paste;
#[doc(hidden)]
pub use sp_core;
#[doc(hidden)]
pub use sp_io;
#[doc(hidden)]
pub use sp_std;
#[doc(hidden)]
pub use codec;

mod bencher;
mod macros;
mod utils;

pub use bencher::*;
pub use utils::*;

#[cfg(feature = "std")]
pub mod bench_runner;
#[cfg(feature = "std")]
pub mod build_wasm;
#[cfg(feature = "std")]
pub mod handler;

#[cfg(feature = "std")]
mod bench_ext;
#[cfg(feature = "std")]
pub mod colorize;
#[cfg(feature = "std")]
mod tracker;

pub use wasm_bencher_procedural::benchmarkable;
