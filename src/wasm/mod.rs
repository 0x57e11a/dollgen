use {
	crate::{format, ErrorKind},
	::convert_case::Casing,
	::serde::Deserialize,
	::std::{
		fs,
		path::{Path, PathBuf},
		process::Command,
	},
	::wasm_bindgen_cli_support::Bindgen,
};

#[derive(Deserialize)]
struct Manifest {
	pub package: ManifestPackage,
}

#[derive(Deserialize)]
struct ManifestPackage {
	pub name: String,
}

fn compile(manifest: PathBuf, release: bool) -> Result<(PathBuf, String), ErrorKind> {
	let src_dir = manifest.parent().unwrap();

	let crate_name = ::toml::from_str::<Manifest>(
		&fs::read_to_string(&manifest).map_err(WASMErrorKind::FailedManifestRead)?,
	)
	.map_err(WASMErrorKind::BadManifest)?
	.package
	.name
	.to_case(::convert_case::Case::Snake);

	let target_dir = Path::new("target/dollgen").join(src_dir);

	// build
	{
		let mut command = Command::new("cargo");

		command
			.arg("build")
			.arg("--manifest-path")
			.arg(
				src_dir
					.join("Cargo.toml")
					.to_str()
					.ok_or(ErrorKind::NonUTF8PathCharacters)?,
			)
			.arg("--target-dir")
			.arg(
				target_dir
					.to_str()
					.ok_or(ErrorKind::NonUTF8PathCharacters)?,
			)
			.arg("--target")
			.arg("wasm32-unknown-unknown");

		if release {
			command.arg("--release");
		}

		let out = command
			.output()
			.map_err(WASMErrorKind::BuildProcessFailed)?;

		if !out.status.success() {
			return Err(WASMErrorKind::BuildFailed(
				String::from_utf8(out.stderr).map_err(WASMErrorKind::NonUTF8Output)?,
			)
			.into());
		}
	}

	// bindgen
	{
		let mut bindgen = Bindgen::new();

		bindgen
			.out_name(&crate_name)
			.input_path(
				target_dir
					.join("wasm32-unknown-unknown")
					.join(if release { "release" } else { "debug" })
					.join(&crate_name)
					.with_extension("wasm")
					.to_str()
					.ok_or(ErrorKind::NonUTF8PathCharacters)?,
			)
			.web(true)
			.map_err(WASMErrorKind::BindgenFailed)?
			.debug(!release)
			.keep_debug(!release)
			.typescript(true);

		bindgen
			.generate(
				target_dir
					.join("bindgen")
					.to_str()
					.ok_or(ErrorKind::NonUTF8PathCharacters)?,
			)
			.map_err(WASMErrorKind::BindgenFailed)?;
	}

	Ok((target_dir.join("bindgen"), crate_name))
}

/// compile rust libraries to wasm and include bindings
///
/// - `release` - whether to compile in release mode
/// - `js` - the [format string](crate::format) to use to determine where to put the js binding file,
///   ultimately you should be importing this in your javascript code
///
/// [see module-level documentation for help](crate::wasm)
pub fn create_wasm_with_bindings(
	release: bool,
	js: &'static str,
) -> impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind> {
	move |src_file, dst_file, cap| {
		let (bindgen_dir, crate_name) = compile(src_file.with_file_name("Cargo.toml"), release)?;

		fs::copy(bindgen_dir.join(format!("{crate_name}_bg.wasm")), &dst_file)?;

		let js_file = &PathBuf::from(format(&js, &cap)?);
		fs::create_dir_all(js_file.parent().unwrap())?;
		fs::copy(bindgen_dir.join(format!("{crate_name}.js")), js_file)?;

		Ok(())
	}
}

/// compile rust libraries to wasm and output the typescript `.d.ts` declaration file for the js module
///
/// - `release` - whether to compile in release mode
///
/// [see module-level documentation for help](crate::wasm)
pub fn create_typescript_declarations(
	release: bool,
) -> impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind> {
	move |src_file, dst_file, _| {
		let (bindgen_dir, crate_name) = compile(src_file.with_file_name("Cargo.toml"), release)?;

		fs::copy(bindgen_dir.join(format!("{crate_name}.d.ts")), &dst_file)?;

		Ok(())
	}
}

/// compile rust libraries to wasm and output the typescript `.d.ts` declaration file for the js module
///
/// - `release` - whether to compile in release mode
///
/// [see module-level documentation for help](crate::wasm)
pub fn create_both(
	release: bool,
	js: &'static str,
	d_ts: &'static str,
) -> impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind> {
	move |src_file, dst_file, cap| {
		let (bindgen_dir, crate_name) = compile(src_file.with_file_name("Cargo.toml"), release)?;

		fs::copy(bindgen_dir.join(format!("{crate_name}_bg.wasm")), &dst_file)?;
		let js_file = &PathBuf::from(format(&js, &cap)?);
		fs::create_dir_all(js_file.parent().unwrap())?;
		fs::copy(bindgen_dir.join(format!("{crate_name}.js")), js_file)?;
		let d_ts_file = &PathBuf::from(format(&d_ts, &cap)?);
		fs::create_dir_all(d_ts_file.parent().unwrap())?;
		fs::copy(bindgen_dir.join(format!("{crate_name}.d.ts")), d_ts_file)?;

		Ok(())
	}
}

/// an error while compiling wasm
#[derive(::thiserror::Error, Debug)]
pub enum WASMErrorKind {
	/// unable to load Cargo.toml manifest
	#[error("failed to read Cargo.toml manifest")]
	FailedManifestRead(#[source] ::std::io::Error),

	/// Cargo.toml manifest invalid
	#[error("bad Cargo.toml manifest")]
	BadManifest(#[source] ::toml::de::Error),

	/// failed to run `cargo build``
	#[error("failed to run cargo build")]
	BuildProcessFailed(#[source] ::std::io::Error),

	/// build failed
	#[error("build failed")]
	BuildFailed(String),

	/// bindgen failed
	#[error("bindgen failed")]
	BindgenFailed(::anyhow::Error),

	/// build output contains non-utf8 characters
	#[error("non utf8 terminal output from build")]
	NonUTF8Output(#[source] ::std::string::FromUtf8Error),
}
