use ::std::{
	ffi::OsStr,
	path::{Path, PathBuf},
};

pub fn with_added_extension_but_stable(path: &Path, extension: impl AsRef<OsStr>) -> PathBuf {
	let mut new = path.extension().unwrap_or_default().to_os_string();
	if path.extension().is_some() {
		new.push(".");
	}
	new.push(extension);
	path.with_extension(new)
}
