use {
	crate::{format, ErrorKind},
	::convert_case::Casing,
	::serde::Deserialize,
	::std::{
		fs,
		path::{Path, PathBuf},
		process::Command,
	},
};

#[derive(Deserialize)]
struct Manifest {
	pub package: ManifestPackage,
}

#[derive(Deserialize)]
struct ManifestPackage {
	pub name: String,
}

pub fn create_wasm_with_bindings(
	release: bool,
	js: &'static str,
) -> impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind> {
	move |src, dst, cap| {
		let src_dir = src.parent().ok_or(WASMErrorKind::NoParent)?;

		let crate_name = ::toml::from_str::<Manifest>(
			&fs::read_to_string(src_dir.join("Cargo.toml"))
				.map_err(WASMErrorKind::FailedManifestRead)?,
		)
		.map_err(WASMErrorKind::BadManifest)?
		.package
		.name
		.to_case(::convert_case::Case::Snake);

		let out_name = dst
			.file_stem()
			.map(|oss| oss.to_str().ok_or(WASMErrorKind::NonUTF8PathCharacters))
			.unwrap_or(Ok("module"))?;

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
						.ok_or(WASMErrorKind::NonUTF8PathCharacters)?,
				)
				.arg("--target-dir")
				.arg(
					target_dir
						.to_str()
						.ok_or(WASMErrorKind::NonUTF8PathCharacters)?,
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
				Err(WASMErrorKind::BuildFailed(
					String::from_utf8(out.stderr).map_err(WASMErrorKind::NonUTF8Output)?,
				))?;
			}
		}

		// bindgen
		{
			let mut command = Command::new("wasm-bindgen");

			command
				.arg("--target")
				.arg("web")
				.arg("--out-dir")
				.arg(
					target_dir
						.join("bindgen")
						.to_str()
						.ok_or(WASMErrorKind::NonUTF8PathCharacters)?,
				)
				.arg("--out-name")
				.arg(out_name)
				.arg(
					target_dir
						.join("wasm32-unknown-unknown")
						.join(if release { "release" } else { "debug" })
						.join(&crate_name)
						.with_extension("wasm")
						.to_str()
						.ok_or(WASMErrorKind::NonUTF8PathCharacters)?,
				);

			let out = command
				.output()
				.map_err(WASMErrorKind::BindgenProcessFailed)?;

			if !out.status.success() {
				Err(WASMErrorKind::BindgenFailed(
					String::from_utf8(out.stderr).map_err(WASMErrorKind::NonUTF8Output)?,
				))?;
			}
		}

		let bindgen_dir = target_dir.join("bindgen");

		let js = PathBuf::from(format(&js, &cap)?);

		fs::copy(bindgen_dir.join(format!("{out_name}_bg.wasm")), &dst)?;
		fs::copy(bindgen_dir.join(format!("{out_name}.js")), &js)?;

		Ok(())
	}
}

#[derive(::thiserror::Error, Debug)]
pub enum WASMErrorKind {
	#[error("selected file has no parent")]
	NoParent,

	#[error("failed to read Cargo.toml manifest")]
	FailedManifestRead(::std::io::Error),

	#[error("bad Cargo.toml manifest")]
	BadManifest(::toml::de::Error),

	#[error("failed to run cargo")]
	BuildProcessFailed(::std::io::Error),

	#[error("build failed")]
	BuildFailed(String),

	#[error("failed to run wasm-bindgen")]
	BindgenProcessFailed(::std::io::Error),

	#[error("bindgen failed")]
	BindgenFailed(String),

	#[error("non utf8 terminal output from build/bindgen")]
	NonUTF8Output(::std::string::FromUtf8Error),

	#[error("non utf8 path characters")]
	NonUTF8PathCharacters,

	#[error(transparent)]
	Other(::anyhow::Error),
}
