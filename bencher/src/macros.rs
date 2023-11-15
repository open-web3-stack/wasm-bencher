/// Run benches in WASM environment.
///
/// Update Cargo.toml by adding:
/// ```toml
/// ..
/// [package]
/// name = "your-module"
/// ..
/// [[bench]]
/// name = 'module_benches'
/// harness = false
/// required-features = ['wasm-bench']
///
/// [features]
/// wasm-bench = [
///    'wasm-bencher/wasm-bench'
///    'weight-meter/wasm-bench'
/// ]
/// ..
/// ```
///
/// Create a file `benches/module_benches.rs` must be the same as bench name.
/// ```.ignore
/// # run benches
/// wasm_bencher::main!();
/// # or, run benches with storage info, required by `wasm-bencher` to generate output json
/// wasm_bencher::main!({ your_module::mock::AllPalletsWithSystem::storage_info() }));
/// ```
///
/// Define benches
///
/// Create a file `src/benches.rs`
/// ```ignore
/// #!#[cfg(feature = "wasm-bench")]
///
/// use wasm_bencher::{Bencher, benches};
/// use crate::mock::*;
///
/// fn foo(b: &mut Bencher) {
///     // Run anything before code here
///     let ret = b.bench(|| {
///         // foo must have macro `[weight_meter::weight(..)]` to measure correct redundant info
///         YourModule::foo()
///     });
///     // Run anything after code here
/// }
///
/// fn bar(b: &mut Bencher) {
///     // optional. method name is used by default i.e: `bar`
///     b.name("bench_name")
///     .bench(|| {
///         // bar must have macro `[weight_meter::weight(..)]` to measure correct redundant info
///         YourModule::bar();
///     });
/// }
///
/// benches!(foo, bar); // Tests are generated automatically
/// ```
/// Update `src/lib.rs`
/// ```ignore
/// #[cfg(any(feature = "wasm-bench", test))]
/// pub mod mock; /* mock runtime needs to be compiled into wasm */
/// pub mod benches;
/// ```
///
/// Run benchmarking: `cargo bench --features=wasm-bench`
/// Run benchmark auto-generated tests: `cargo test --features=wasm-bench`
#[macro_export]
macro_rules! benches {
    ($($method:path),+) => {
        #[cfg(feature = "wasm-bench")]
        $crate::paste::item! {
            use $crate::sp_std::vec::Vec;
            $crate::sp_core::wasm_export_functions! {
                // list of bench methods
                fn available_bench_methods() -> Vec<&str> {
                    let mut methods = Vec::<&str>::new();
                    $(
                        methods.push(stringify!($method));
                    )+
                    methods.sort();
                    methods
                }

                // wrapped bench methods to run
                $(
                    fn [<bench_ $method>] () -> $crate::Bencher {
                        let name = stringify!($method);
                        let mut bencher = $crate::Bencher::with_name(name);

                        for _ in 0..1_000 {
                            bencher.before_run();
                            $method(&mut bencher);
                        }

                        bencher
                    }
                )+
            }
        }


        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        #[panic_handler]
        fn panic_handler(info: &::core::panic::PanicInfo) -> ! {
            let message = $crate::sp_std::alloc::format!("{}", info);
            $crate::bench::print_error(message.as_bytes().to_vec());
            unsafe {core::arch::wasm32::unreachable(); }
        }

        // Tests
        #[cfg(test)]
        mod tests {
            $(
                $crate::paste::item! {
                    #[test]
                    fn [<bench_ $method>] () {
                        ::sp_io::TestExternalities::new_empty().execute_with(|| {
                            let mut bencher = $crate::Bencher::default();
                            super::$method(&mut bencher);
                        });
                    }
                }
            )+
        }

    }
}

#[macro_export]
macro_rules! main {
	(
        $($storage_info:block)?
    ) => {
		#[cfg(all(feature = "std", feature = "wasm-bench"))]
		pub fn main() -> std::io::Result<()> {
            // build project to wasm
			let wasm = $crate::build_wasm::build()?;

            // get list of bench methods
            let methods = $crate::bench_runner::run(&wasm[..], "available_bench_methods", &[]).unwrap();
            let bench_methods = <Vec<String> as $crate::codec::Decode>::decode(&mut &methods[..]).unwrap();
            println!("\nRunning {} benches\n", bench_methods.len());

            let mut results: Vec<$crate::handler::BenchData> = vec![];
            let mut failed: Vec<String> = vec![];

            // bench each method
            for method in bench_methods {
                $crate::handler::print_start(&method);
                match $crate::bench_runner::run(&wasm[..], &format!("bench_{method}"), &[])
                {
                    Ok(output) => {
                        let data = $crate::handler::parse(output);
                        $crate::handler::print_summary(&data);
                        results.push(data);
                    }
                    Err(err) => {
                        failed.push(method);
                    }
                };
            }

            // print summary
            if failed.is_empty() {
                println!("\n✅ Complete: {}", $crate::colorize::green_bold(&format!("{} passed", results.len())));
            } else {
                println!("\n❌ Finished with errors: {}, {}", $crate::colorize::green_bold(&format!("{} passed", results.len())), $crate::colorize::red_bold(&format!("{} failed", failed.len())));
                std::process::exit(1);
            }

            // save output to json if `json` arg is passed
            if std::env::args().find(|x| x.eq("json")).is_some() {
                use ::frame_support::traits::StorageInfoTrait;
                let mut storage_info: Vec<::frame_support::traits::StorageInfo> = vec![];
                $(storage_info = $storage_info;)?
                assert!(!storage_info.is_empty(), "Cannot find storage info, please include `AllPalletsWithSystem` generated by `frame_support::construct_runtime`");
                $crate::handler::save_output_json(results, storage_info.into_iter().map(|x| {
                    $crate::handler::StorageMetadata {
                        pallet_name: String::from_utf8_lossy(&x.pallet_name).to_string(),
                        storage_name: String::from_utf8_lossy(&x.storage_name).to_string(),
                        prefix: x.prefix,
                        max_values: x.max_values,
                        max_size: x.max_size,
                    }
                }).collect());
            }

			Ok(())
		}
	};
}
