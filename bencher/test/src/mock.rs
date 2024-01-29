#![cfg(any(test, feature = "wasm-bench"))]

use frame_support::derive_impl;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	MultiSignature,
};
use sp_std::prelude::*;

pub type Signature = MultiSignature;
pub type BlockNumber = u64;
pub type AccountId = u32;
pub type Address = sp_runtime::MultiAddress<AccountId, u32>;
pub type Header = sp_runtime::generic::Header<BlockNumber, BlakeTwo256>;

pub type SignedExtra = (frame_system::CheckWeight<Runtime>,);

pub type UncheckedExtrinsic =
	sp_runtime::generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Test: crate::pallet,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

impl crate::pallet::Config for Runtime {}

#[cfg(test)]
#[derive(Default)]
pub struct ExtBuilder;

#[cfg(test)]
impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		use sp_runtime::BuildStorage;
		frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.unwrap()
			.into()
	}
}
