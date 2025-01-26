use {
	::dollgen::liquid::liquid::partials::PartialSource,
	::std::{borrow::Cow, fs, path::Path},
};

#[derive(Debug)]
pub struct FsPartialSource;

impl PartialSource for FsPartialSource {
	fn contains(&self, name: &str) -> bool {
		Path::new(name).is_file()
	}

	fn names(&self) -> Vec<&str> {
		Vec::new()
	}

	fn try_get<'a>(&'a self, name: &str) -> Option<Cow<'a, str>> {
		let path = Path::new(name);
		if fs::exists(path).unwrap_or(false) {
			Some(Cow::Owned(fs::read_to_string(path).unwrap()))
		} else {
			None
		}
	}
}
