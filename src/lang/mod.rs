//! source languages for templating rules

use {
	crate::ErrorKind,
	::core::cell::RefCell,
	::std::{path::Path, rc::Rc},
};

#[cfg(feature = "lang-markdoll")]
pub mod markdoll;

/// wraps a language parser so it can be easily shared between multiple rules
pub fn shared_lang(
	lang: impl for<'a> FnMut(&'a str, &'a Path) -> Result<(String, String), ErrorKind>,
) -> impl for<'a> FnMut(&'a str, &'a Path) -> Result<(String, String), ErrorKind> + Clone {
	let lang = Rc::new(RefCell::new(lang));

	move |src, path| lang.borrow_mut()(src, path)
}

/// errors parsing template source languages
#[derive(::thiserror::Error, ::miette::Diagnostic, Debug)]
pub enum LangErrorKind {
	/// markdoll error
	///
	/// requires `lang-markdoll` feature
	#[cfg(feature = "lang-markdoll")]
	#[error("markdoll failed ({} errors)", .0)]
	#[diagnostic(code(dollgen::lang::markdoll))]
	Markdoll(usize),
}
