use {
	crate::ErrorKind,
	::grass::{from_path, Options},
	::std::{fs, path::PathBuf},
	core::result::Result,
};

pub extern crate grass;

/// compiles scss/sass
///
/// - `options` - the options to compile with
pub fn create(
	options: Options<'static>,
) -> impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind> {
	move |src, dst, _| {
		fs::write(dst, from_path(src, &options).map_err(|err| *err)?)?;

		Ok(())
	}
}
