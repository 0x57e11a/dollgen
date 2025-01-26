use {
	crate::{format, ErrorKind, PlannedTransformation},
	::convert_case::Casing,
	::serde::Deserialize,
	::std::{
		fs,
		path::{Path, PathBuf},
		process::Command,
	},
	::tracing::{debug_span, instrument, trace, trace_span, Level},
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

#[instrument(level = Level::TRACE)]
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
		let _trace_span = trace_span!("cargo build", ?manifest, ?target_dir).entered();

		let mut command = Command::new("cargo");

		command
			.arg("build")
			.arg("--manifest-path")
			.arg(manifest.to_str().ok_or(ErrorKind::NonUTF8PathCharacters)?)
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
			let stderr = String::from_utf8(out.stderr).unwrap();
			return Err(WASMErrorKind::BuildFailed {
				span: (0, stderr.len()),
				stderr,
			}
			.into());
		}
	}

	// bindgen
	{
		let input = target_dir
			.join("wasm32-unknown-unknown")
			.join(if release { "release" } else { "debug" })
			.join(&crate_name)
			.with_extension("wasm");
		let bindgen_target = target_dir.join("bindgen");

		let _trace_span = trace_span!("wasm-bindgen", ?input, ?bindgen_target).entered();

		let mut bindgen = Bindgen::new();

		bindgen
			.out_name(&crate_name)
			.input_path(input.to_str().ok_or(ErrorKind::NonUTF8PathCharacters)?)
			.web(true)
			.map_err(WASMErrorKind::BindgenFailed)?
			.debug(!release)
			.keep_debug(!release)
			.typescript(true);

		bindgen
			.generate(
				bindgen_target
					.to_str()
					.ok_or(ErrorKind::NonUTF8PathCharacters)?,
			)
			.map_err(|err| WASMErrorKind::BindgenFailed(err.into()))?;
	}

	Ok((target_dir.join("bindgen"), crate_name))
}

#[derive(Debug)]
pub struct WASMPlan {
	pub bindgen_dir: PathBuf,
	pub crate_name: String,
	pub kind: WASMPlanKind,
}

#[derive(Debug)]
pub enum WASMPlanKind {
	Wasm { js: PathBuf },
	TypescriptDeclarations,
	Both { js: PathBuf, d_ts: PathBuf },
}

impl PlannedTransformation for WASMPlan {
	#[instrument(name = "wasm", level = Level::DEBUG)]
	fn execute(self: Box<Self>, dst_file: PathBuf) -> Result<(), ErrorKind> {
		match &self.kind {
			WASMPlanKind::Wasm { js } | WASMPlanKind::Both { js, .. } => {
				let from = self
					.bindgen_dir
					.join(format!("{}_bg.wasm", self.crate_name));
				let to = &dst_file;
				trace!(?from, ?to, ".wasm");
				fs::copy(from, to)?;

				let from = self.bindgen_dir.join(format!("{}.js", self.crate_name));
				let to = js;
				trace!(?from, ?to, ".js");
				fs::create_dir_all(to.parent().unwrap())?;
				fs::copy(from, to)?;
			}
			_ => {}
		}

		match &self.kind {
			WASMPlanKind::TypescriptDeclarations => {
				let from = self.bindgen_dir.join(format!("{}.d.ts", self.crate_name));
				let to = &dst_file;
				trace!(?from, ?to, ".d.ts");
				fs::copy(from, to)?;
			}
			WASMPlanKind::Both { d_ts, .. } => {
				let from = self.bindgen_dir.join(format!("{}.d.ts", self.crate_name));
				let to = d_ts;
				trace!(?from, ?to, ".d.ts");
				fs::create_dir_all(to.parent().unwrap())?;
				fs::copy(from, to)?;
			}
			_ => {}
		}

		Ok(())
	}
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
) -> impl FnMut(PathBuf, Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	move |src_file, cap| {
		let _trace_span = debug_span!("wasm", ?release, ?js).entered();

		let (bindgen_dir, crate_name) = compile(src_file.with_file_name("Cargo.toml"), release)?;

		Ok(Box::new(WASMPlan {
			bindgen_dir,
			crate_name,
			kind: WASMPlanKind::Wasm {
				js: PathBuf::from(format(&js, &cap)?),
			},
		}))
	}
}

/// compile rust libraries to wasm and output the typescript `.d.ts` declaration file for the js module
///
/// - `release` - whether to compile in release mode
///
/// [see module-level documentation for help](crate::wasm)
pub fn create_typescript_declarations(
	release: bool,
) -> impl FnMut(PathBuf, Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	move |src_file, _| {
		let _trace_span = debug_span!("typescript declarations", ?release).entered();

		let (bindgen_dir, crate_name) = compile(src_file.with_file_name("Cargo.toml"), release)?;

		Ok(Box::new(WASMPlan {
			bindgen_dir,
			crate_name,
			kind: WASMPlanKind::TypescriptDeclarations,
		}))
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
) -> impl FnMut(PathBuf, Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	move |src_file, cap| {
		let _trace_span = debug_span!("wasm + typescript declarations", ?release, ?js).entered();

		let (bindgen_dir, crate_name) = compile(src_file.with_file_name("Cargo.toml"), release)?;

		Ok(Box::new(WASMPlan {
			bindgen_dir,
			crate_name,
			kind: WASMPlanKind::Both {
				js: PathBuf::from(format(&js, &cap)?),
				d_ts: PathBuf::from(format(&d_ts, &cap)?),
			},
		}))
	}
}

/// an error while compiling wasm
#[derive(::thiserror::Error, ::miette::Diagnostic, Debug)]
pub enum WASMErrorKind {
	/// unable to load Cargo.toml manifest
	#[error("failed to read Cargo.toml manifest")]
	#[diagnostic(
		code(dollgen::wasm::manifest::read),
		help("the build file should be in the same directory as the manifest")
	)]
	FailedManifestRead(#[source] ::std::io::Error),

	/// Cargo.toml manifest invalid
	#[error("bad Cargo.toml manifest")]
	#[diagnostic(code(dollgen::wasm::manifest::parse))]
	BadManifest(#[source] ::toml::de::Error),

	/// failed to run `cargo build``
	#[error("failed to run `cargo build`")]
	#[diagnostic(
		code(dollgen::wasm::build::process_fail),
		help("is `cargo` on the PATH?")
	)]
	BuildProcessFailed(#[source] ::std::io::Error),

	/// build failed
	#[error("build failed")]
	#[diagnostic(code(dollgen::wasm::build::fail), help("stderr provided"))]
	BuildFailed {
		/// the standard error output of the build
		#[source_code]
		stderr: String,
		/// spans from the start to the end of `stderr`, used for miette diagnostics
		#[label]
		span: (usize, usize),
	},

	/// bindgen failed
	#[error("bindgen failed")]
	#[diagnostic(code(dollgen::wasm::bindgen::fail))]
	BindgenFailed(#[source] ::anyhow::Error),
}
