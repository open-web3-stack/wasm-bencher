use super::{write_file_if_changed, CargoCommandVersioned, OFFLINE};

use cargo_metadata::{CargoOpt, Metadata, MetadataCommand};
use parity_wasm::elements::{deserialize_buffer, Module};
use std::{
	borrow::ToOwned,
	collections::HashSet,
	env, fs,
	hash::{Hash, Hasher},
	ops::Deref,
	path::{Path, PathBuf},
	process,
};
use strum::{EnumIter, IntoEnumIterator};
use toml::value::Table;

/// Colorize an info message.
///
/// Returns the colorized message.
fn colorize_info_message(message: &str) -> String {
	if crate::colorize::color_output_enabled() {
		ansi_term::Color::Yellow.bold().paint(message).to_string()
	} else {
		message.into()
	}
}

/// Holds the path to the bloaty WASM binary.
pub struct WasmBinaryBloaty(PathBuf);

impl WasmBinaryBloaty {
	/// Returns the escaped path to the bloaty wasm binary.
	pub fn wasm_binary_bloaty_path_escaped(&self) -> String {
		self.0.display().to_string().escape_default().to_string()
	}

	/// Returns the path to the wasm binary.
	pub fn wasm_binary_bloaty_path(&self) -> &Path {
		&self.0
	}
}

/// Holds the path to the WASM binary.
pub struct WasmBinary(PathBuf);

impl WasmBinary {
	/// Returns the path to the wasm binary.
	pub fn wasm_binary_path(&self) -> &Path {
		&self.0
	}

	/// Returns the escaped path to the wasm binary.
	pub fn wasm_binary_path_escaped(&self) -> String {
		self.0.display().to_string().escape_default().to_string()
	}
}

fn crate_metadata(cargo_manifest: &Path) -> Metadata {
	let mut cargo_lock = cargo_manifest.to_path_buf();
	cargo_lock.set_file_name("Cargo.lock");

	let cargo_lock_existed = cargo_lock.exists();

	// If we can find a `Cargo.lock`, we assume that this is the workspace root and there exists a
	// `Cargo.toml` that we can use for getting the metadata.
	let cargo_manifest = if let Some(mut cargo_lock) = find_cargo_lock(cargo_manifest) {
		cargo_lock.set_file_name("Cargo.toml");
		cargo_lock
	} else {
		cargo_manifest.to_path_buf()
	};

	let mut crate_metadata_command = create_metadata_command(cargo_manifest);
	crate_metadata_command.features(CargoOpt::AllFeatures);

	let crate_metadata = crate_metadata_command
		.exec()
		.expect("`cargo metadata` can not fail on project `Cargo.toml`; qed");
	// If the `Cargo.lock` didn't exist, we need to remove it after
	// calling `cargo metadata`. This is required to ensure that we don't change
	// the build directory outside of the `target` folder. Commands like
	// `cargo publish` require this.
	if !cargo_lock_existed {
		let _ = fs::remove_file(&cargo_lock);
	}

	crate_metadata
}

/// Creates the WASM project, compiles the WASM binary and compacts the WASM binary.
///
/// # Returns
///
/// The path to the compact WASM binary and the bloaty WASM binary.
pub fn create_and_compile(
	project_cargo_toml: &Path,
	default_rustflags: &str,
	cargo_cmd: CargoCommandVersioned,
	features_to_enable: Vec<String>,
	wasm_binary_name: Option<String>,
	check_for_runtime_version_section: bool,
) -> (Option<WasmBinary>, WasmBinaryBloaty) {
	let wasm_workspace_root = get_wasm_workspace_root();
	let wasm_workspace = wasm_workspace_root.join("wbuild");

	let crate_metadata = crate_metadata(project_cargo_toml);

	let project = create_project(
		project_cargo_toml,
		&wasm_workspace,
		&crate_metadata,
		crate_metadata.workspace_root.as_ref(),
		features_to_enable,
	);

	let profile = build_project(&project, default_rustflags, cargo_cmd);
	let (wasm_binary, wasm_binary_compressed, bloaty) =
		compact_wasm_file(&project, profile, project_cargo_toml, wasm_binary_name);

	if check_for_runtime_version_section {
		ensure_runtime_version_wasm_section_exists(bloaty.wasm_binary_bloaty_path());
	}

	if let Some(wasm_binary) = wasm_binary.as_ref() {
		copy_wasm_to_target_directory(project_cargo_toml, wasm_binary)
	}

	if let Some(wasm_binary_compressed) = wasm_binary_compressed.as_ref() {
		copy_wasm_to_target_directory(project_cargo_toml, wasm_binary_compressed)
	}

	let final_wasm_binary = wasm_binary_compressed.or(wasm_binary);

	(final_wasm_binary, bloaty)
}

/// Ensures that the `runtime_version` wasm section exists in the given wasm file.
///
/// If the section can not be found, it will print an error and exit the builder.
fn ensure_runtime_version_wasm_section_exists(wasm: &Path) {
	let wasm_blob = fs::read(wasm).expect("`{wasm}` was just written and should exist; qed");

	let module: Module = match deserialize_buffer(&wasm_blob) {
		Ok(m) => m,
		Err(e) => {
			println!("Failed to deserialize `{}`: {e:?}", wasm.display());
			process::exit(1);
		},
	};

	if !module.custom_sections().any(|cs| cs.name() == "runtime_version") {
		println!(
			"Couldn't find the `runtime_version` wasm section. \
				  Please ensure that you are using the `sp_version::runtime_version` attribute macro!"
		);
		process::exit(1);
	}
}

/// Find the `Cargo.lock` relative to the `OUT_DIR` environment variable.
///
/// If the `Cargo.lock` cannot be found, we emit a warning and return `None`.
fn find_cargo_lock(cargo_manifest: &Path) -> Option<PathBuf> {
	fn find_impl(mut path: PathBuf) -> Option<PathBuf> {
		loop {
			if path.join("Cargo.lock").exists() {
				return Some(path.join("Cargo.lock"));
			}

			if !path.pop() {
				return None;
			}
		}
	}

	if let Ok(workspace) = env::var(super::WASM_BUILD_WORKSPACE_HINT) {
		let path = PathBuf::from(workspace);

		if path.join("Cargo.lock").exists() {
			return Some(path.join("Cargo.lock"));
		} else {
			build_helper::warning!(
				"`{}` env variable doesn't point to a directory that contains a `Cargo.lock`.",
				super::WASM_BUILD_WORKSPACE_HINT,
			);
		}
	}

	if let Some(path) = find_impl(build_helper::out_dir()) {
		return Some(path);
	}

	build_helper::warning!(
		"Could not find `Cargo.lock` for `{}`, while searching from `{}`. \
		 To fix this, point the `{}` env variable to the directory of the workspace being compiled.",
		cargo_manifest.display(),
		build_helper::out_dir().display(),
		super::WASM_BUILD_WORKSPACE_HINT,
	);

	None
}

/// Extract the crate name from the given `Cargo.toml`.
fn get_crate_name(cargo_manifest: &Path) -> String {
	let cargo_toml: Table = toml::from_str(
		&fs::read_to_string(cargo_manifest).expect("File exists as checked before; qed"),
	)
	.expect("Cargo manifest is a valid toml file; qed");

	let package = cargo_toml
		.get("package")
		.and_then(|t| t.as_table())
		.expect("`package` key exists in valid `Cargo.toml`; qed");

	package
		.get("name")
		.and_then(|p| p.as_str())
		.map(ToOwned::to_owned)
		.expect("Package name exists; qed")
}

/// Returns the name for the wasm binary.
fn get_wasm_binary_name(cargo_manifest: &Path) -> String {
	get_crate_name(cargo_manifest).replace('-', "_")
}

/// Returns the root path of the wasm workspace.
fn get_wasm_workspace_root() -> PathBuf {
	let mut out_dir = build_helper::out_dir();

	loop {
		match out_dir.parent() {
			Some(parent) if out_dir.ends_with("build") => return parent.to_path_buf(),
			_ => {
				if !out_dir.pop() {
					break;
				}
			},
		}
	}

	panic!(
		"Could not find target dir in: {}",
		build_helper::out_dir().display()
	)
}

fn create_project_cargo_toml(
	wasm_workspace: &Path,
	workspace_root_path: &Path,
	crate_name: &str,
	crate_path: &Path,
	wasm_binary: &str,
	enabled_features: impl Iterator<Item = String>,
) {
	let mut workspace_toml: Table = toml::from_str(
		&fs::read_to_string(workspace_root_path.join("Cargo.toml"))
			.expect("Workspace root `Cargo.toml` exists; qed"),
	)
	.expect("Workspace root `Cargo.toml` is a valid toml file; qed");

	let mut wasm_workspace_toml = Table::new();

	// Add different profiles which are selected by setting `WASM_BUILD_TYPE`.
	let mut release_profile = Table::new();
	release_profile.insert("panic".into(), "abort".into());
	release_profile.insert("lto".into(), "thin".into());

	let mut production_profile = Table::new();
	production_profile.insert("inherits".into(), "release".into());
	production_profile.insert("lto".into(), "fat".into());
	production_profile.insert("codegen-units".into(), 1.into());

	let mut dev_profile = Table::new();
	dev_profile.insert("panic".into(), "abort".into());

	let mut profile = Table::new();
	profile.insert("release".into(), release_profile.into());
	profile.insert("production".into(), production_profile.into());
	profile.insert("dev".into(), dev_profile.into());

	wasm_workspace_toml.insert("profile".into(), profile.into());

	// Add patch section from the project root `Cargo.toml`
	while let Some(mut patch) =
		workspace_toml.remove("patch").and_then(|p| p.try_into::<Table>().ok())
	{
		// Iterate over all patches and make the patch path absolute from the workspace root path.
		patch
			.iter_mut()
			.filter_map(|p| {
				p.1.as_table_mut().map(|t| t.iter_mut().filter_map(|t| t.1.as_table_mut()))
			})
			.flatten()
			.for_each(|p| {
				p.iter_mut().filter(|(k, _)| k == &"path").for_each(|(_, v)| {
					if let Some(path) = v.as_str().map(PathBuf::from) {
						if path.is_relative() {
							*v = workspace_root_path.join(path).display().to_string().into();
						}
					}
				})
			});

		wasm_workspace_toml.insert("patch".into(), patch.into());
	}

	let mut package = Table::new();
	package.insert("name".into(), format!("{}-wasm", crate_name).into());
	package.insert("version".into(), "1.0.0".into());
	package.insert("edition".into(), "2021".into());

	wasm_workspace_toml.insert("package".into(), package.into());

	let mut lib = Table::new();
	lib.insert("name".into(), wasm_binary.into());
	lib.insert("crate-type".into(), vec!["cdylib".to_string()].into());

	wasm_workspace_toml.insert("lib".into(), lib.into());

	let mut dependencies = Table::new();

	let mut wasm_project = Table::new();
	wasm_project.insert("package".into(), crate_name.into());
	wasm_project.insert("path".into(), crate_path.display().to_string().into());
	wasm_project.insert("default-features".into(), false.into());
	wasm_project.insert(
		"features".into(),
		enabled_features.collect::<Vec<_>>().into(),
	);

	dependencies.insert("wasm-project".into(), wasm_project.into());

	wasm_workspace_toml.insert("dependencies".into(), dependencies.into());

	wasm_workspace_toml.insert("workspace".into(), Table::new().into());

	write_file_if_changed(
		wasm_workspace.join("Cargo.toml"),
		toml::to_string_pretty(&wasm_workspace_toml).expect("Wasm workspace toml is valid; qed"),
	);
}

/// Find a package by the given `manifest_path` in the metadata. In case it can't be found by its
/// manifest_path, fallback to finding it by name; this is necessary during publish because the
/// package's manifest path will be *generated* within a specific packaging directory, thus it won't
/// be found by its original path anymore.
///
/// Panics if the package could not be found.
fn find_package_by_manifest_path<'a>(
	pkg_name: &str,
	manifest_path: &Path,
	crate_metadata: &'a cargo_metadata::Metadata,
) -> &'a cargo_metadata::Package {
	if let Some(pkg) = crate_metadata.packages.iter().find(|p| p.manifest_path == manifest_path) {
		return pkg;
	}

	let pkgs_by_name = crate_metadata
		.packages
		.iter()
		.filter(|p| p.name == pkg_name)
		.collect::<Vec<_>>();

	if let Some(pkg) = pkgs_by_name.first() {
		if pkgs_by_name.len() > 1 {
			panic!(
				"Found multiple packages matching the name {pkg_name} ({manifest_path:?}): {:?}",
				pkgs_by_name
			);
		}
		pkg
	} else {
		panic!("Failed to find entry for package {pkg_name} ({manifest_path:?}).");
	}
}

/// Get a list of enabled features for the project.
fn project_enabled_features(
	pkg_name: &str,
	cargo_manifest: &Path,
	crate_metadata: &cargo_metadata::Metadata,
) -> Vec<String> {
	let package = find_package_by_manifest_path(pkg_name, cargo_manifest, crate_metadata);

	let std_enabled = package.features.get("std");

	let mut enabled_features = package
		.features
		.iter()
		.filter(|(f, v)| {
			let mut feature_env = f.replace('-', "_");
			feature_env.make_ascii_uppercase();

			// If this is a feature that corresponds only to an optional dependency
			// and this feature is enabled by the `std` feature, we assume that this
			// is only done through the `std` feature. This is a bad heuristic and should
			// be removed after namespaced features are landed:
			// https://doc.rust-lang.org/cargo/reference/unstable.html#namespaced-features
			// Then we can just express this directly in the `Cargo.toml` and do not require
			// this heuristic anymore. However, for the transition phase between now and namespaced
			// features already being present in nightly, we need this code to make
			// runtimes compile with all the possible rustc versions.
			if v.len() == 1
				&& v.get(0).map_or(false, |v| *v == format!("dep:{}", f))
				&& std_enabled.as_ref().map(|e| e.iter().any(|ef| ef == *f)).unwrap_or(false)
			{
				return false;
			}

			// We don't want to enable the `std`/`default` feature for the wasm build and
			// we need to check if the feature is enabled by checking the env variable.
			*f != "std"
				&& *f != "default"
				&& env::var(format!("CARGO_FEATURE_{}", feature_env))
					.map(|v| v == "1")
					.unwrap_or_default()
		})
		.map(|d| d.0.clone())
		.collect::<Vec<_>>();

	enabled_features.sort();
	enabled_features
}

/// Returns if the project has the `runtime-wasm` feature
fn has_runtime_wasm_feature_declared(
	pkg_name: &str,
	cargo_manifest: &Path,
	crate_metadata: &cargo_metadata::Metadata,
) -> bool {
	let package = find_package_by_manifest_path(pkg_name, cargo_manifest, crate_metadata);

	package.features.keys().any(|k| k == "runtime-wasm")
}

/// Create the project used to build the wasm binary.
///
/// # Returns
///
/// The path to the created wasm project.
fn create_project(
	project_cargo_toml: &Path,
	wasm_workspace: &Path,
	crate_metadata: &Metadata,
	workspace_root_path: &Path,
	features_to_enable: Vec<String>,
) -> PathBuf {
	let crate_name = get_crate_name(project_cargo_toml);
	let crate_path = project_cargo_toml.parent().expect("Parent path exists; qed");
	let wasm_binary = get_wasm_binary_name(project_cargo_toml);
	let wasm_project_folder = wasm_workspace.join(&crate_name);

	fs::create_dir_all(wasm_project_folder.join("src"))
		.expect("Wasm project dir create can not fail; qed");

	let mut enabled_features =
		project_enabled_features(&crate_name, project_cargo_toml, crate_metadata);

	if has_runtime_wasm_feature_declared(&crate_name, project_cargo_toml, crate_metadata) {
		enabled_features.push("runtime-wasm".into());
	}

	let mut enabled_features = enabled_features.into_iter().collect::<HashSet<_>>();
	enabled_features.extend(features_to_enable);

	create_project_cargo_toml(
		&wasm_project_folder,
		workspace_root_path,
		&crate_name,
		crate_path,
		&wasm_binary,
		enabled_features.into_iter(),
	);

	write_file_if_changed(
		wasm_project_folder.join("src/lib.rs"),
		"#![no_std] pub use wasm_project::*;",
	);

	if let Some(crate_lock_file) = find_cargo_lock(project_cargo_toml) {
		// Use the `Cargo.lock` of the main project.
		super::copy_file_if_changed(crate_lock_file, wasm_project_folder.join("Cargo.lock"));
	}

	wasm_project_folder
}

/// The cargo profile that is used to build the wasm project.
#[derive(Debug, EnumIter)]
pub enum Profile {
	/// The `--profile dev` profile.
	Debug,
	/// The `--profile release` profile.
	Release,
	/// The `--profile production` profile.
	Production,
}

impl Profile {
	/// Create a profile by detecting which profile is used for the main build.
	///
	/// We cannot easily determine the profile that is used by the main cargo invocation
	/// because the `PROFILE` environment variable won't contain any custom profiles like
	/// "production". It would only contain the builtin profile where the custom profile
	/// inherits from. This is why we inspect the build path to learn which profile is used.
	///
	/// # Note
	///
	/// Can be overriden by setting [`super::WASM_BUILD_TYPE_ENV`].
	pub fn detect(wasm_project: &Path) -> Profile {
		let (name, overriden) = if let Ok(name) = env::var(super::WASM_BUILD_TYPE_ENV) {
			(name, true)
		} else {
			// First go backwards to the beginning of the target directory.
			// Then go forwards to find the "wbuild" directory.
			// We need to go backwards first because when starting from the root there
			// might be a chance that someone has a "wbuild" directory somewhere in the path.
			let name = wasm_project
				.components()
				.rev()
				.take_while(|c| c.as_os_str() != "target")
				.collect::<Vec<_>>()
				.iter()
				.rev()
				.take_while(|c| c.as_os_str() != "wbuild")
				.last()
				.expect("We put the wasm project within a `target/.../wbuild` path; qed")
				.as_os_str()
				.to_str()
				.expect("All our profile directory names are ascii; qed")
				.to_string();
			(name, false)
		};
		match (Profile::iter().find(|p| p.directory() == name), overriden) {
			// When not overriden by a env variable we default to using the `Release` profile
			// for the wasm build even when the main build uses the debug build. This
			// is because the `Debug` profile is too slow for normal development activities.
			(Some(Profile::Debug), false) => Profile::Release,
			// For any other profile or when overriden we take it at face value.
			(Some(profile), _) => profile,
			// For non overriden unknown profiles we fall back to `Release`.
			// This allows us to continue building when a custom profile is used for the
			// main builds cargo. When explicitly passing a profile via env variable we are
			// not doing a fallback.
			(None, false) => {
				let profile = Profile::Release;
				build_helper::warning!(
					"Unknown cargo profile `{}`. Defaulted to `{:?}` for the runtime build.",
					name,
					profile,
				);
				profile
			},
			// Invalid profile specified.
			(None, true) => {
				// We use println! + exit instead of a panic in order to have a cleaner output.
				println!(
					"Unexpected profile name: `{}`. One of the following is expected: {:?}",
					name,
					Profile::iter().map(|p| p.directory()).collect::<Vec<_>>(),
				);
				process::exit(1);
			},
		}
	}

	/// The name of the profile as supplied to the cargo `--profile` cli option.
	pub fn name(&self) -> &'static str {
		match self {
			Self::Debug => "dev",
			Self::Release => "release",
			Self::Production => "production",
		}
	}

	/// The sub directory within `target` where cargo places the build output.
	///
	/// # Note
	///
	/// Usually this is the same as [`Self::name`] with the exception of the debug
	/// profile which is called `dev`.
	fn directory(&self) -> &'static str {
		match self {
			Self::Debug => "debug",
			_ => self.name(),
		}
	}

	/// Whether the resulting binary should be compacted and compressed.
	fn wants_compact(&self) -> bool {
		!matches!(self, Self::Debug)
	}
}

/// Check environment whether we should build without network
fn offline_build() -> bool {
	env::var(OFFLINE).map_or(false, |v| v == "true")
}

/// Build the project to create the WASM binary.
fn build_project(
	project: &Path,
	default_rustflags: &str,
	cargo_cmd: CargoCommandVersioned,
) -> Profile {
	let manifest_path = project.join("Cargo.toml");
	let mut build_cmd = cargo_cmd.command();

	let rustflags = format!(
		"-C target-cpu=mvp -C target-feature=-sign-ext -C link-arg=--export-table {} {}",
		default_rustflags,
		env::var(super::WASM_BUILD_RUSTFLAGS_ENV).unwrap_or_default(),
	);

	build_cmd
		.args(["rustc", "--target=wasm32-unknown-unknown"])
		.arg(format!("--manifest-path={}", manifest_path.display()))
		.env("RUSTFLAGS", rustflags)
		// Unset the `CARGO_TARGET_DIR` to prevent a cargo deadlock (cargo locks a target dir
		// exclusive). The runner project is created in `CARGO_TARGET_DIR` and executing it will
		// create a sub target directory inside of `CARGO_TARGET_DIR`.
		.env_remove("CARGO_TARGET_DIR")
		// As we are being called inside a build-script, this env variable is set. However, we set
		// our own `RUSTFLAGS` and thus, we need to remove this. Otherwise cargo favors this
		// env variable.
		.env_remove("CARGO_ENCODED_RUSTFLAGS")
		// We don't want to call ourselves recursively
		.env(super::SKIP_BUILD_ENV, "");

	if crate::colorize::color_output_enabled() {
		build_cmd.arg("--color=always");
	}

	let profile = Profile::detect(project);
	build_cmd.arg("--profile");
	build_cmd.arg(profile.name());

	if offline_build() {
		build_cmd.arg("--offline");
	}

	println!(
		"{}",
		colorize_info_message("Information that should be included in a bug report.")
	);
	println!(
		"{} {:?}",
		colorize_info_message("Executing build command:"),
		build_cmd
	);
	println!(
		"{} {}",
		colorize_info_message("Using rustc version:"),
		cargo_cmd.rustc_version()
	);

	match build_cmd.status().map(|s| s.success()) {
		Ok(true) => profile,
		// Use `process.exit(1)` to have a clean error output.
		_ => process::exit(1),
	}
}

/// Compact the WASM binary using `wasm-gc` and compress it using zstd.
fn compact_wasm_file(
	project: &Path,
	profile: Profile,
	cargo_manifest: &Path,
	out_name: Option<String>,
) -> (Option<WasmBinary>, Option<WasmBinary>, WasmBinaryBloaty) {
	let default_out_name = get_wasm_binary_name(cargo_manifest);
	let out_name = out_name.unwrap_or_else(|| default_out_name.clone());
	let in_path = project
		.join("target/wasm32-unknown-unknown")
		.join(profile.directory())
		.join(format!("{}.wasm", default_out_name));

	let (wasm_compact_path, wasm_compact_compressed_path) = if profile.wants_compact() {
		let wasm_compact_path = project.join(format!("{}.compact.wasm", out_name,));
		wasm_opt::OptimizationOptions::new_opt_level_0()
			.mvp_features_only()
			.debug_info(true)
			.add_pass(wasm_opt::Pass::StripDwarf)
			.run(&in_path, &wasm_compact_path)
			.expect("Failed to compact generated WASM binary.");

		let wasm_compact_compressed_path =
			project.join(format!("{}.compact.compressed.wasm", out_name));
		if compress_wasm(&wasm_compact_path, &wasm_compact_compressed_path) {
			(
				Some(WasmBinary(wasm_compact_path)),
				Some(WasmBinary(wasm_compact_compressed_path)),
			)
		} else {
			(Some(WasmBinary(wasm_compact_path)), None)
		}
	} else {
		(None, None)
	};

	let bloaty_path = project.join(format!("{}.wasm", out_name));
	fs::copy(in_path, &bloaty_path).expect("Copying the bloaty file to the project dir.");

	(
		wasm_compact_path,
		wasm_compact_compressed_path,
		WasmBinaryBloaty(bloaty_path),
	)
}

fn compress_wasm(wasm_binary_path: &Path, compressed_binary_out_path: &Path) -> bool {
	use sp_maybe_compressed_blob::CODE_BLOB_BOMB_LIMIT;

	let data = fs::read(wasm_binary_path).expect("Failed to read WASM binary");
	if let Some(compressed) = sp_maybe_compressed_blob::compress(&data, CODE_BLOB_BOMB_LIMIT) {
		fs::write(compressed_binary_out_path, &compressed[..])
			.expect("Failed to write WASM binary");

		true
	} else {
		build_helper::warning!(
			"Writing uncompressed wasm. Exceeded maximum size {}",
			CODE_BLOB_BOMB_LIMIT,
		);

		false
	}
}

/// Custom wrapper for a [`cargo_metadata::Package`] to store it in
/// a `HashSet`.
#[derive(Debug)]
struct DeduplicatePackage<'a> {
	package: &'a cargo_metadata::Package,
	identifier: String,
}

impl<'a> From<&'a cargo_metadata::Package> for DeduplicatePackage<'a> {
	fn from(package: &'a cargo_metadata::Package) -> Self {
		Self {
			package,
			identifier: format!("{}{}{:?}", package.name, package.version, package.source),
		}
	}
}

impl<'a> Hash for DeduplicatePackage<'a> {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.identifier.hash(state);
	}
}

impl<'a> PartialEq for DeduplicatePackage<'a> {
	fn eq(&self, other: &Self) -> bool {
		self.identifier == other.identifier
	}
}

impl<'a> Eq for DeduplicatePackage<'a> {}

impl<'a> Deref for DeduplicatePackage<'a> {
	type Target = cargo_metadata::Package;

	fn deref(&self) -> &Self::Target {
		self.package
	}
}

fn create_metadata_command(path: impl Into<PathBuf>) -> MetadataCommand {
	let mut metadata_command = MetadataCommand::new();
	metadata_command.manifest_path(path);

	if offline_build() {
		metadata_command.other_options(vec!["--offline".to_owned()]);
	}
	metadata_command
}

/// Copy the WASM binary to the target directory set in `WASM_TARGET_DIRECTORY` environment
/// variable. If the variable is not set, this is a no-op.
fn copy_wasm_to_target_directory(cargo_manifest: &Path, wasm_binary: &WasmBinary) {
	let target_dir = match env::var(super::WASM_TARGET_DIRECTORY) {
		Ok(path) => PathBuf::from(path),
		Err(_) => return,
	};

	if !target_dir.is_absolute() {
		// We use println! + exit instead of a panic in order to have a cleaner output.
		println!(
			"Environment variable `{}` with `{}` is not an absolute path!",
			super::WASM_TARGET_DIRECTORY,
			target_dir.display(),
		);
		process::exit(1);
	}

	fs::create_dir_all(&target_dir).expect("Creates `WASM_TARGET_DIRECTORY`.");

	fs::copy(
		wasm_binary.wasm_binary_path(),
		target_dir.join(format!("{}.wasm", get_wasm_binary_name(cargo_manifest))),
	)
	.expect("Copies WASM binary to `WASM_TARGET_DIRECTORY`.");
}
