#![doc = include_str!("../README.doll")]
#![warn(
	clippy::pedantic,
	clippy::allow_attributes_without_reason,
	missing_docs
)]
#![allow(clippy::missing_errors_doc, reason = "a lot of ")]

pub use ::capturing_glob::{Entry, Pattern};
use {
	::capturing_glob::{glob_with, MatchOptions},
	::miette::{Diagnostic, NamedSource, SourceSpan},
	::std::{
		collections::HashSet,
		fs,
		path::{Path, PathBuf},
	},
	::strfmt::{strfmt_map, DisplayStr, FmtError, Formatter},
	::tracing::{debug_span, error, info_span, instrument, Level},
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

#[cfg(feature = "minijinja")]
pub mod minijinja;

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

pub mod lang;

mod util;

/// the core of dollgen, defines a list of globs to include, a list of globs to exclude, how to transform the file, and where to emit it to
#[::tyfling::debug(
	"+ {:?}\n- {:?}\n> \"{dst}\"",
	include.iter().map(ToString::to_string).collect::<Vec<_>>(),
	exclude.iter().map(ToString::to_string).collect::<Vec<_>>()
)]
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
	/// plan a transformation
	///
	/// takes the input path (matched by an `include`), and the captures from the `include` that matched
	///
	/// returns plan data to be passed into `execute`
	pub plan: &'a mut dyn FnMut(
		PathBuf,
		Vec<String>,
	) -> Result<Box<dyn PlannedTransformation>, ErrorKind>,
}

/// a planned transformation that can be `execute`d
///
/// this trait can be downcasted to access the internal plan (this is useful for those that want to plan transformations and peek/modify them before executing)
pub trait PlannedTransformation: ::core::any::Any + ::core::fmt::Debug {
	/// executes the planned transformation
	///
	/// takes the plan data and output path (produced by `dst`)
	///
	/// # Errors
	///
	/// if the execution fails
	fn execute(self: Box<Self>, dst: PathBuf) -> Result<(), ErrorKind>;
}

/// [noop] transformation, does not write to the destination file
impl PlannedTransformation for () {
	fn execute(self: Box<Self>, _: PathBuf) -> Result<(), ErrorKind> {
		Ok(())
	}
}

/// writes the binary blob to the destination file
impl PlannedTransformation for Vec<u8> {
	#[instrument(skip(self), name = "write binary blob", level = Level::DEBUG)]
	fn execute(self: Box<Self>, dst: PathBuf) -> Result<(), ErrorKind> {
		fs::write(dst, *self).map_err(ErrorKind::Io)
	}
}

/// writes the string to the destination file
impl PlannedTransformation for String {
	#[instrument(skip(self), name = "write string data", level = Level::DEBUG)]
	fn execute(self: Box<Self>, dst: PathBuf) -> Result<(), ErrorKind> {
		fs::write(dst, self.as_bytes()).map_err(ErrorKind::Io)
	}
}

/// [copy] transformation, copies the file path specified to the destination file
impl PlannedTransformation for PathBuf {
	#[instrument(name = "copy", level = Level::DEBUG)]
	fn execute(self: Box<Self>, dst: PathBuf) -> Result<(), ErrorKind> {
		fs::copy(*self, dst).map_err(ErrorKind::Io)?;
		Ok(())
	}
}

/// a plan to transform a file
#[derive(Debug)]
pub struct Plan {
	/// the destination file
	pub dst: PathBuf,
	/// the plan data produced by the `plan` function
	pub data: Box<dyn PlannedTransformation>,
}

///
///
/// equivalent to `execute(plan(rules)?)`
pub fn run(rules: &mut [Rule<'_>]) -> Result<(), ErrorKind> {
	execute(plan(rules)?)
}

/// plan
#[instrument(skip(rules))]
pub fn plan(rules: &mut [Rule<'_>]) -> Result<Vec<Plan>, ErrorKind> {
	let mut plans = Vec::new();
	let mut visited = HashSet::new();

	for (rule_index, rule) in rules.iter_mut().enumerate() {
		let _span = debug_span!("rule", rule_index, ?rule).entered();

		for (include_index, include) in rule.include.iter().enumerate() {
			let _span =
				debug_span!("include", include_index, include = include.to_string()).entered();

			for entry in glob_with(
				include.as_str(),
				&MatchOptions {
					case_sensitive: true,
					require_literal_leading_dot: false,
					require_literal_separator: true,
				},
			)
			.map_err(|err| ErrorKind::Pattern {
				label: [::miette::LabeledSpan::new_primary_with_span(
					Some(err.msg.to_string()),
					SourceSpan::new(err.pos.into(), 1),
				)],
				src: NamedSource::new(
					format!("rules[{rule_index}].include[{include_index}]"),
					include.to_string(),
				),
			})? {
				let entry = entry?;
				let src_file = entry.path();

				// pull captures out into a vec
				let captures = {
					let mut captures = Vec::new();

					let mut i = 1; // skip 0, which is just the entire match
					while let Some(capture) = entry.group(i) {
						i += 1;
						captures.push(
							capture
								.to_str()
								.ok_or(ErrorKind::NonUTF8PathCharacters)?
								.to_string(),
						);
					}

					captures
				};

				let dst_file = format(rule.dst, &captures)?;
				let dst_file = Path::new(&*dst_file);

				let _span = info_span!(
					"plan file",
					src = src_file.to_str().unwrap(),
					dst = dst_file.to_str().unwrap()
				)
				.entered();

				// make sure it isnt excluded and that it hasn't been visited yet

				if !src_file.is_file() {
					error!("skipped (not a file)");
					continue;
				}

				if visited.contains(src_file) {
					error!("skipped (already visited)");
					continue;
				}

				if rule.exclude.iter().any(|ignore| {
					if ignore.matches_path(src_file) {
						error!("skipped (matched ignore)");
						true
					} else {
						false
					}
				}) {
					continue;
				}

				plans.push(Plan {
					dst: dst_file.to_path_buf(),
					data: (rule.plan)(src_file.to_path_buf(), captures)?,
				});

				visited.insert(src_file.to_path_buf());
			}
		}
	}

	Ok(plans)
}

#[instrument(skip(plans))]
pub fn execute(plans: Vec<Plan>) -> Result<(), ErrorKind> {
	for plan in plans {
		// ensure the directory is there
		fs::create_dir_all(plan.dst.parent().unwrap())?;

		plan.data.execute(plan.dst)?;
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
#[instrument(level = Level::DEBUG)]
pub fn noop(_: PathBuf, _: Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	Ok(Box::new(()))
}

/// a primitive transformer that just [`fs::copy`]'s its input path to its output path
#[instrument(level = Level::DEBUG)]
pub fn copy(src: PathBuf, _: Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	Ok(Box::new(src))
}

/// an error
#[derive(::thiserror::Error, ::miette::Diagnostic, Debug)]
pub enum ErrorKind {
	/// parsing failure
	#[error("pattern failure to compile")]
	#[diagnostic(code(dollgen::glob::bad_pattern))]
	Pattern {
		#[label(collection)]
		label: [::miette::LabeledSpan; 1],
		#[source_code]
		src: ::miette::NamedSource<String>,
	},

	/// searching failure
	#[error("glob failure")]
	#[diagnostic(code(dollgen::glob::failure))]
	Glob(
		#[source]
		#[from]
		::capturing_glob::GlobError,
	),

	/// an error while formatting a format-string
	#[error("failure to parse format string")]
	#[diagnostic(code(dollgen::format_str))]
	Format(
		#[source]
		#[from]
		::strfmt::FmtError,
	),

	/// liquid integration failure
	///
	/// requires `liquid` feature
	#[cfg(feature = "liquid")]
	#[error("liquid integration failure")]
	#[diagnostic(code(dollgen::liquid))]
	LiquidIntegration(
		#[source]
		#[from]
		liquid::LiquidErrorKind,
	),

	/// minijinja integration failure
	///
	/// requires `minijinja` feature
	#[cfg(feature = "minijinja")]
	#[error("minijinja integration failure")]
	#[diagnostic(code(dollgen::minijinja))]
	MinijinjaIntegration(
		#[source]
		#[from]
		minijinja::MinijinjaErrorKind,
	),

	/// scss integration failure
	///
	/// requires `scss` feature
	#[cfg(feature = "scss")]
	#[error("scss integration failure")]
	#[diagnostic(code(dollgen::scss))]
	SCSSIntegration {
		/// the section of the source that failure
		#[label(collection)]
		span: [::miette::LabeledSpan; 1],
		/// the source file that failure
		#[source_code]
		src: ::miette::NamedSource<String>,
	},

	/// wasm integration failure
	///
	/// requires `wasm` feature
	#[cfg(feature = "wasm")]
	#[error("wasm integration failure")]
	#[diagnostic(code(dollgen::wasm))]
	WASMIntegration(
		#[source]
		#[from]
		wasm::WASMErrorKind,
	),

	/// template source lang failure
	#[error("template source lang failure")]
	#[diagnostic(code(dollgen::lang))]
	Lang(
		#[source]
		#[from]
		lang::LangErrorKind,
	),

	/// filesystem failure
	#[error("fs error")]
	#[diagnostic(code(dollgen::io))]
	Io(
		#[source]
		#[from]
		::std::io::Error,
	),

	/// a path contained non-utf8 characters
	#[error("non-utf8 path characters")]
	#[diagnostic(code(dollgen::io::non_utf8_path))]
	NonUTF8PathCharacters,

	/// a file contained non-utf8 characters
	#[error("non-utf8 content")]
	#[diagnostic(code(dollgen::io::non_utf8_content))]
	NonUTF8Characters,

	/// something else
	#[error("other")]
	#[diagnostic(transparent)]
	Other(#[source] Box<dyn Diagnostic + Send + Sync>),
}
