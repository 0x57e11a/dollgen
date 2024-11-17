pub use ::capturing_glob::{Entry, Pattern};
use {
	::capturing_glob::{glob_with, MatchOptions},
	::std::{
		collections::HashSet,
		fs::{self, create_dir_all},
		path::{Path, PathBuf},
	},
	::strfmt::{strfmt_map, DisplayStr, FmtError, Formatter},
};

#[cfg(feature = "liquid")]
pub mod liquid;

#[cfg(feature = "scss")]
pub mod scss;

pub struct Rule<'a> {
	pub include: &'a [Pattern],
	pub exclude: &'a [Pattern],
	pub dst: &'a str,
	pub transformer: &'a mut dyn FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind>,
}

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

				if src_file.is_file()
					&& !visited.contains(src_file)
					&& rule
						.exclude
						.iter()
						.all(|ignore| !ignore.matches_path(src_file))
				{
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

					create_dir_all(dst_file.parent().unwrap()).unwrap();

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

pub fn copy(src: PathBuf, dst: PathBuf, _: Vec<String>) -> Result<(), ErrorKind> {
	fs::copy(src, dst)?;
	Ok(())
}

#[derive(::thiserror::Error, Debug)]
pub struct Error {
	#[source]
	pub kind: ErrorKind,
	pub rule: usize,
	pub include: usize,
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

#[derive(::thiserror::Error, Debug)]
pub enum ErrorKind {
	// core
	#[error("pattern failed to compile")]
	Pattern(#[from] ::capturing_glob::PatternError),

	#[error("glob failed")]
	Glob(#[from] ::capturing_glob::GlobError),

	#[error(transparent)]
	Io(#[from] ::std::io::Error),

	#[error("non-utf8 path characters")]
	NonUTF8PathCharacters,

	#[error("failed to parse format string")]
	Format(#[from] ::strfmt::FmtError),

	// integrations
	#[cfg(feature = "liquid")]
	#[error("liquid integration failed")]
	LiquidIntegration(#[from] liquid::LiquidErrorKind),

	#[cfg(feature = "scss")]
	#[error("scss integration failed")]
	SCSSIntegration(#[from] grass::Error),

	// misc
	#[error(transparent)]
	Other(anyhow::Error),
}
