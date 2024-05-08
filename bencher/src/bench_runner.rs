use super::{
	bench_ext::BenchExt,
	tracker::{BenchTracker, BenchTrackerExt},
};
use frame_support::sp_runtime::traits::HashingFor;
use sc_executor::WasmExecutor;
use sc_executor_common::runtime_blob::RuntimeBlob;
use sp_externalities::Extensions;
use sp_state_machine::{Ext, OverlayedChanges};
use sp_std::sync::Arc;

type Header =
	frame_support::sp_runtime::generic::Header<u32, frame_support::sp_runtime::traits::BlakeTwo256>;
type Block =
	frame_support::sp_runtime::generic::Block<Header, frame_support::sp_runtime::OpaqueExtrinsic>;

type ComposeHostFunctions = (
	crate::sp_io::SubstrateHostFunctions,
	super::bench::HostFunctions,
);

fn executor() -> WasmExecutor<ComposeHostFunctions> {
	WasmExecutor::<ComposeHostFunctions>::builder()
		.with_max_runtime_instances(1)
		.with_runtime_cache_size(0)
		.build()
}

/// Run benches
pub fn run(
	wasm_code: &[u8],
	method: &str,
	call_data: &[u8],
) -> Result<Vec<u8>, sc_executor_common::error::Error> {
	let mut overlay = OverlayedChanges::default();

	let state =
		sc_client_db::BenchmarkingState::<HashingFor<Block>>::new(Default::default(), None, true, true)?;

	let tracker = Arc::new(BenchTracker::new());
	let tracker_ext = BenchTrackerExt(Arc::clone(&tracker));

	let mut extensions = Extensions::default();
	extensions.register(tracker_ext);

	let ext = Ext::<_, _>::new(&mut overlay, &state, Some(&mut extensions));
	let mut bench_ext = BenchExt::new(ext, tracker);

	let blob = RuntimeBlob::uncompress_if_needed(wasm_code)?;

	executor().uncached_call(blob, &mut bench_ext, false, method, call_data)
}
