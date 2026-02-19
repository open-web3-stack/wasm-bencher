use sp_runtime_interface::runtime_interface;
use sp_runtime_interface::pass_by::AllocateAndReturnByCodec;
use sp_runtime_interface::pass_by::PassFatPointerAndRead;
use sp_std::vec::Vec;

#[cfg(feature = "std")]
use super::colorize::red_bold;
#[cfg(feature = "std")]
use super::tracker::BenchTrackerExt;
#[cfg(feature = "std")]
use sp_externalities::ExternalitiesExt;

#[runtime_interface]
pub trait Bench {
	fn print_error(message: PassFatPointerAndRead<Vec<u8>>) {
		#[cfg(feature = "std")]
		{
			let msg = String::from_utf8_lossy(&message);
			eprintln!("{}", red_bold(&msg));
		}
	}

	fn warnings(&mut self) -> AllocateAndReturnByCodec<Vec<u8>> {
		let tracker = &***self
			.extension::<BenchTrackerExt>()
			.expect("No `bench_tracker` associated for the current context!");
		tracker.warnings()
	}

	fn commit_db(&mut self) {
		self.commit()
	}

	fn wipe_db(&mut self) {
		self.wipe()
	}

	fn reset_read_write_count(&mut self) {
		self.reset_read_write_count()
	}

	fn start_timer(&mut self) {
		let tracker = &***self
			.extension::<BenchTrackerExt>()
			.expect("No `bench_tracker` associated for the current context!");
		tracker.prepare_next_run();
		tracker.instant();
	}

	fn end_timer(&mut self) -> u64 {
		let tracker = &***self
			.extension::<BenchTrackerExt>()
			.expect("No `bench_tracker` associated for the current context!");
		tracker.elapsed() as u64
	}

	fn before_block(&mut self) {
		let tracker = &***self
			.extension::<BenchTrackerExt>()
			.expect("No `bench_tracker` associated for the current context!");
		tracker.before_block();
	}

	fn after_block(&mut self) {
		let tracker = &***self
			.extension::<BenchTrackerExt>()
			.expect("No `bench_tracker` associated for the current context!");
		tracker.after_block();
	}

	fn redundant_time(&mut self) -> u64 {
		let tracker = &***self
			.extension::<BenchTrackerExt>()
			.expect("No `bench_tracker` associated for the current context!");
		tracker.redundant_time() as u64
	}

	fn read_written_keys(&mut self) -> AllocateAndReturnByCodec<Vec<u8>> {
		let tracker = &***self
			.extension::<BenchTrackerExt>()
			.expect("No `bench_tracker` associated for the current context!");
		tracker.read_written_keys()
	}

	fn whitelist(&mut self, key: PassFatPointerAndRead<Vec<u8>>, read: bool, write: bool) {
		let tracker = &***self
			.extension::<BenchTrackerExt>()
			.expect("No `bench_tracker` associated for the current context!");
		tracker.whitelist(key, read, write);
	}
}
