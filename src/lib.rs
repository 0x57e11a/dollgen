#![doc = include_str!("../README.doll")]
#![warn(
	clippy::pedantic,
	clippy::allow_attributes_without_reason,
	missing_docs
)]

pub use ::capturing_glob::{Entry, Pattern};
use {
	::capturing_glob::{glob_with, MatchOptions},
	::std::{
		collections::HashSet,
		fs,
		path::{Path, PathBuf},
	},
	::strfmt::{strfmt_map, DisplayStr, FmtError, Formatter},
};

/// compile liquid templates, based on input languages
///
/// languages parse their source code and may provide a frontmatter string, which is parsed as TOML:
///
/// - `template` (optional)
///   - if `template.local` is true:
///     - if `template.path` is defined, that template is used, and the path is assumed to be relative to the directory containing the source file
///     - if `template.path` is not defined, it uses the template with the same name as the source file (ex: `page.doll` will use `page.liquid` in the same directory)
///   - if `template.local` is false or not specified:
///     - if `template.path` is defined, that template is used, and the path is assumed to be relative to the root of the build
///     - if `template.path` is not defined, the default template is used
/// - `props` (optional)
///   - values are fed into the liquid template
///
/// requires `liquid` feature
#[cfg(feature = "liquid")]
pub mod liquid;

/// compile scss/sass stylesheets to css
///
/// requires `scss` feature
#[cfg(feature = "scss")]
pub mod scss;

/// compile rust source code libraries to wasm files with an accompanying javascript file to load them
///
/// requires `wasm` feature
#[cfg(feature = "wasm")]
pub mod wasm;

/// the core of dollgen, defines a list of globs to include, a list of globs to exclude, how to transform the file, and where to emit it to
pub struct Rule<'a> {
	/// which files to include
	///
	/// may capture parts of the path (ex: `src/**/*.doll`)
	pub include: &'a [Pattern],
	/// which files to exclude
	pub exclude: &'a [Pattern],
	/// where output files should be emitted
	///
	/// format specifiers like `{0}` pull from the captures of whatever `include` glob matched (ex: `dist/{0}/{1}.html`)
	pub dst: &'static str,
	/// transform an input file to an output file
	///
	/// takes the input path (matched by an `include`), output path (produced by `dst`), and the captures from the `include` that matched
	pub transformer: &'a mut dyn FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind>,
}

/// the main function, which takes the rules and searches files, transforming them when they match a rule
///
/// rules are parsed in order, so the first rule will always take precedent if it matches
pub fn run(rules: &mut [Rule<'_>]) -> Result<(), Error> {
	let mut visited = HashSet::new();

	for (rule_index, rule) in rules.iter_mut().enumerate() {
		for (include_index, include) in rule.include.iter().enumerate() {
			for entry in glob_with(
				include.as_str(),
				&MatchOptions {
					case_sensitive: true,
					require_literal_leading_dot: true,
					require_literal_separator: true,
				},
			)
			.map_err(|kind| Error {
				kind: kind.into(),
				rule: rule_index,
				include: include_index,
				file: None,
			})? {
				let entry = entry.map_err(|kind| Error {
					kind: kind.into(),
					rule: rule_index,
					include: include_index,
					file: None,
				})?;
				let src_file = entry.path();

				// make sure it isnt excluded an that it hasn't been visited yet
				if src_file.is_file()
					&& !visited.contains(src_file)
					&& rule
						.exclude
						.iter()
						.all(|ignore| !ignore.matches_path(src_file))
				{
					// pull captures out into a vec
					let captures = {
						let mut captures = Vec::new();

						let mut i = 1; // skip 0, which is just the entire match
						while let Some(capture) = entry.group(i) {
							i += 1;
							captures.push(
								capture
									.to_str()
									.ok_or_else(|| Error {
										kind: ErrorKind::NonUTF8PathCharacters,
										rule: rule_index,
										include: include_index,
										file: Some(src_file.to_path_buf()),
									})?
									.to_string(),
							);
						}

						captures
					};
					let dst_file = format(rule.dst, &captures).map_err(|kind| Error {
						kind: kind.into(),
						rule: rule_index,
						include: include_index,
						file: Some(src_file.to_path_buf()),
					})?;
					let dst_file = Path::new(&*dst_file);

					// ensure the directory is there
					fs::create_dir_all(dst_file.parent().unwrap()).map_err(|err| Error {
						kind: err.into(),
						rule: rule_index,
						include: include_index,
						file: Some(src_file.to_path_buf()),
					})?;

					(rule.transformer)(src_file.to_path_buf(), dst_file.to_path_buf(), captures)
						.map_err(|kind| Error {
							kind: kind.into(),
							rule: rule_index,
							include: include_index,
							file: Some(src_file.to_path_buf()),
						})?;

					visited.insert(src_file.to_path_buf());
				}
			}
		}
	}

	Ok(())
}

/// quickly format a format-string with a given set of captures
///
/// ex: `dist/{0}/{1}.html`
pub fn format<T: AsRef<str>>(fmt: &str, captures: &[T]) -> Result<String, ErrorKind> {
	Ok(strfmt_map(fmt, |mut fmt: Formatter| {
		captures
			.get(
				fmt.key
					.parse::<usize>()
					.map_err(|_| FmtError::KeyError(format!("non-numeric key: \"{}\"", fmt.key)))?,
			)
			.ok_or_else(|| FmtError::KeyError(format!("key {} out of range", fmt.key)))?
			.as_ref()
			.display_str(&mut fmt)
	})?)
}

/// the most primitive transformer, does absolutely nothing
pub fn noop(_: PathBuf, _: PathBuf, _: Vec<String>) -> Result<(), ErrorKind> {
	Ok(())
}

/// a primitive transformer that just [`fs::copy`]'s its input path to its output path
pub fn copy(src: PathBuf, dst: PathBuf, _: Vec<String>) -> Result<(), ErrorKind> {
	fs::copy(src, dst)?;
	Ok(())
}

/// an error with context
#[derive(::thiserror::Error, Debug)]
pub struct Error {
	/// the actual error that occurred
	#[source]
	pub kind: ErrorKind,
	/// the index of the rule where this error occurred
	pub rule: usize,
	/// the index of the include in the rule where this error occurred
	pub include: usize,
	/// if available, the specific input file that was being processed when this error occurred
	pub file: Option<PathBuf>,
}

impl core::fmt::Display for Error {
	fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(fmt, "rule #{}, include #{}", self.rule, self.include)?;

		if let Some(file) = &self.file {
			write!(fmt, ", file {}", file.to_string_lossy())?;
		}

		write!(fmt, ": ")?;

		self.kind.fmt(fmt)?;

		writeln!(fmt)
	}
}

/// an error
#[derive(::thiserror::Error, Debug)]
pub enum ErrorKind {
	/// parsing failure
	#[error("pattern failed to compile")]
	Pattern(#[from] ::capturing_glob::PatternError),

	/// searching failure
	#[error("glob failed")]
	Glob(#[from] ::capturing_glob::GlobError),

	/// filesystem failure
	#[error(transparent)]
	Io(#[from] ::std::io::Error),

	/// a path contained non-utf8 characters
	#[error("non-utf8 path characters")]
	NonUTF8PathCharacters,

	/// an error while formatting a format-string
	#[error("failed to parse format string")]
	Format(#[from] ::strfmt::FmtError),

	/// liquid integration failure
	///
	/// requires `liquid` feature
	#[cfg(feature = "liquid")]
	#[error("liquid integration failed")]
	LiquidIntegration(#[from] liquid::LiquidErrorKind),

	/// scss integration failure
	///
	/// requires `scss` feature
	#[cfg(feature = "scss")]
	#[error("scss integration failed")]
	SCSSIntegration(#[from] ::grass::Error),

	/// wasm integration failure
	///
	/// requires `wasm` feature
	#[cfg(feature = "wasm")]
	#[error("wasm integration failed")]
	WASMIntegration(#[from] wasm::WASMErrorKind),

	/// something else
	#[error(transparent)]
	Other(::anyhow::Error),
}
