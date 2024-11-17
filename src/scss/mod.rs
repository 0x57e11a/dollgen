use {
	crate::ErrorKind,
	::codemap::SpanLoc,
	::grass::{from_path, Logger, Options},
	::std::{fs, path::PathBuf},
	core::result::Result,
};

pub extern crate grass;

#[inline]
pub fn logger() -> &'static dyn Logger {
	#[derive(Debug)]
	struct AnnotatedLogger;

	impl Logger for AnnotatedLogger {
		#[inline]
		fn debug(&self, location: SpanLoc, message: &str) {
			eprintln!(
				"[dollgen::scss | DEBUG in {}:{}:{}] {}",
				location.file.name(),
				location.begin.line + 1,
				location.begin.column + 1,
				message
			);
		}

		#[inline]
		fn warn(&self, location: SpanLoc, message: &str) {
			eprintln!(
				"[dollgen::scss | WARN in {}:{}:{}] {}",
				location.file.name(),
				location.begin.line + 1,
				location.begin.column + 1,
				message
			);
		}
	}

	&AnnotatedLogger
}

pub fn create(
	options: Options<'static>,
) -> impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind> {
	move |src, dst, _| {
		fs::write(dst, from_path(src, &options).map_err(|err| *err)?)?;

		Ok(())
	}
}
