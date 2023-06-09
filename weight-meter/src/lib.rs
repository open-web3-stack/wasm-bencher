#![cfg_attr(not(feature = "std"), no_std)]

//! 1. Add macro attribute on method you want to benchmark.
//! ```ignore
//! #[weight_meter::weight(0)]
//! fn inner_do_something(something: u32) {
//!     // Update storage.
//!     Something::<T>::put(something);
//! }
//! ```
//! Start with `0` and after the weights is generated then it can be replaced
//! with generated weight. Macro will inject callable methods that wraps inner
//! methods. This only works for methods with `weight_meter::start` and
//! `weight_meter::weight` attribute, and only when running benchmarks.
//!
//! 2. Create benchmarks using wasm_bencher and generate the weights with weight_gen
//! After running the benchmarks and the weights have been generated then we can
//! replace
//! ```ignore
//! #[weight_meter::weight(0)]
//! ```
//! with
//!```ignore
//! #[weight_meter::weight(ModuleWeights::<T>::inner_do_something())]
//! ```
//!
//! 3. Use WeightMeter on your calls by adding macro
//! `#[weight_meter::start(weight)]` or `#[weight_meter::start]` if
//! starts with zero and at the end use `weight_meter::used_weight()` to
//! get used weight. ```ignore
//! #[pallet::call]
//! impl<T: Config> Pallet<T> {
//!     #[pallet::weight(T::WeightInfo::do_something())]
//!     #[weight_meter::start(ModuleWeights::<T>::do_something())]
//!     pub fn do_something(origin: OriginFor<T>, something: u32) ->
//!     DispatchResultWithPostInfo {
//!         let who = ensure_signed(origin)?;
//!         Self::inner_do_something(something);
//!         // Emit an event.
//!         Self::deposit_event(Event::SomethingStored(something, who));
//!         Ok(PostDispatchInfo::from(Some(weight_meter::used_weight())))
//!     }
//! }
//! ```

type Weight = u64;

struct Meter {
	used_weight: Weight,
	// Depth gets incremented when entering call or a sub-call
	// This is used to avoid miscalculation during sub-calls
	depth: u8,
}

mod meter_no_std;
mod meter_std;

extern crate self as weight_meter;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
pub use meter_std::*;

#[cfg(not(feature = "std"))]
pub use meter_no_std::*;

/// Start weight meter
pub use weight_meter_procedural::start;

/// Measure each methods weight
pub use weight_meter_procedural::weight;
