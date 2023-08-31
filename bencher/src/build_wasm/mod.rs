use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::{
	fs,
	path::{Path, PathBuf},
	process::Command,
};

mod prerequisites;
mod wasm_project;

/// Environment variable that tells us to skip building the wasm binary.
const SKIP_BUILD_ENV: &str = "SKIP_WASM_BUILD";

/// Environment variable that tells us whether we should avoid network requests
const OFFLINE: &str = "CARGO_NET_OFFLINE";

/// Environment variable to force a certain build type when building the wasm binary.
/// Expects "debug", "release" or "production" as value.
///
/// When unset the WASM binary uses the same build type as the main cargo build with
/// the exception of a debug build: In this case the wasm build defaults to `release` in
/// order to avoid a slowdown when not explicitly requested.
const WASM_BUILD_TYPE_ENV: &str = "WASM_BUILD_TYPE";

/// Environment variable to extend the `RUSTFLAGS` variable given to the wasm build.
const WASM_BUILD_RUSTFLAGS_ENV: &str = "WASM_BUILD_RUSTFLAGS";

/// Environment variable to set the target directory to copy the final wasm binary.
///
/// The directory needs to be an absolute path.
const WASM_TARGET_DIRECTORY: &str = "WASM_TARGET_DIRECTORY";

/// Environment variable that hints the workspace we are building.
const WASM_BUILD_WORKSPACE_HINT: &str = "WASM_BUILD_WORKSPACE_HINT";

/// Write to the given `file` if the `content` is different.
fn write_file_if_changed(file: impl AsRef<Path>, content: impl AsRef<str>) {
	if fs::read_to_string(file.as_ref()).ok().as_deref() != Some(content.as_ref()) {
		fs::write(file.as_ref(), content.as_ref())
			.unwrap_or_else(|_| panic!("Writing `{}` can not fail!", file.as_ref().display()));
	}
}

/// Copy `src` to `dst` if the `dst` does not exist or is different.
fn copy_file_if_changed(src: PathBuf, dst: PathBuf) {
	let src_file = fs::read_to_string(&src).ok();
	let dst_file = fs::read_to_string(&dst).ok();

	if src_file != dst_file {
		fs::copy(&src, &dst).unwrap_or_else(|_| {
			panic!(
				"Copying `{}` to `{}` can not fail; qed",
				src.display(),
				dst.display()
			)
		});
	}
}

/// Wraps a specific command which represents a cargo invocation.
#[derive(Debug)]
pub struct CargoCommand {
	program: String,
}

impl CargoCommand {
	fn new(program: &str) -> Self {
		CargoCommand {
			program: program.into(),
		}
	}

	fn command(&self) -> Command {
		Command::new(&self.program)
	}
}

/// Wraps a [`CargoCommand`] and the version of `rustc` the cargo command uses.
pub struct CargoCommandVersioned {
	command: CargoCommand,
	version: String,
}

impl CargoCommandVersioned {
	pub fn new(command: CargoCommand, version: String) -> Self {
		Self { command, version }
	}

	/// Returns the `rustc` version.
	pub fn rustc_version(&self) -> &str {
		&self.version
	}
}

impl std::ops::Deref for CargoCommandVersioned {
	type Target = CargoCommand;

	fn deref(&self) -> &CargoCommand {
		&self.command
	}
}

pub fn build() -> std::io::Result<Vec<u8>> {
	let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let pkg_name = std::env::var("CARGO_PKG_NAME").unwrap();

	let random = thread_rng()
		.sample_iter(&Alphanumeric)
		.take(16)
		.map(char::from)
		.collect::<String>();

	let profile = wasm_project::Profile::detect(&std::env::current_dir()?).name();
	let mut out_dir = std::path::PathBuf::from(manifest_dir);
	out_dir.push(format!("target/{profile}/build/{pkg_name}-{random}/out"));

	std::env::set_var("OUT_DIR", out_dir.display().to_string());

	let mut project_cargo_toml = std::env::current_dir()?;
	project_cargo_toml.push("Cargo.toml");

	let default_rustflags = "-Clink-arg=--export=__heap_base -C link-arg=--import-memory";
	let cargo_cmd = match prerequisites::check() {
		Ok(cmd) => cmd,
		Err(err_msg) => {
			eprintln!("{}", err_msg);
			std::process::exit(1);
		},
	};

	let (wasm_binary, bloaty) = wasm_project::create_and_compile(
		&project_cargo_toml,
		default_rustflags,
		cargo_cmd,
		vec!["wasm-bench".to_string()],
		None,
		false,
	);

	let (wasm_binary, _wasm_binary_bloaty) = if let Some(wasm_binary) = wasm_binary {
		(
			wasm_binary.wasm_binary_path_escaped(),
			bloaty.wasm_binary_bloaty_path_escaped(),
		)
	} else {
		(
			bloaty.wasm_binary_bloaty_path_escaped(),
			bloaty.wasm_binary_bloaty_path_escaped(),
		)
	};

	let bytes = std::fs::read(wasm_binary)?;

	Ok(bytes.to_vec())
}
