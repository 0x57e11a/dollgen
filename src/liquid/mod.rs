use {
	crate::ErrorKind,
	::core::cell::RefCell,
	::hashbrown::{hash_map::Entry, HashMap},
	::liquid::{object, Object, Parser, ParserBuilder, Template},
	::serde::Deserialize,
	::std::{
		ffi::OsStr,
		fs::{self, OpenOptions},
		path::{Path, PathBuf},
		rc::Rc,
	},
	::toml::from_str,
};

/// [markdoll](https://codeberg.org/0x57e11a/markdoll) support
///
/// requires `liquid-markdoll` feature
#[cfg(feature = "liquid-markdoll")]
pub mod markdoll;

pub extern crate liquid;

/// parses and cahces liquid templates
///
/// ensure to [`clear_cache`](Liquid::clear_cache) in case templates change
pub struct Liquid {
	/// the parser
	pub parser: Parser,
	cache: HashMap<PathBuf, Template>,
}

impl Liquid {
	/// create from a liquid parser builder
	pub fn new(pb: ParserBuilder) -> Result<Rc<RefCell<Self>>, ErrorKind> {
		Ok(Rc::new(RefCell::new(Self {
			parser: pb
				.build()
				.map_err(|err| LiquidErrorKind::Liquid(err, None))?,
			cache: HashMap::new(),
		})))
	}

	/// parse a template file or retrieve from cache
	pub fn parse(&mut self, path: &Path) -> Result<&Template, ErrorKind> {
		Ok(match self.cache.entry(path.to_path_buf()) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => entry.insert(
				self.parser
					.parse_file(path)
					.map_err(|err| LiquidErrorKind::Liquid(err, Some(path.to_path_buf())))?,
			),
		})
	}

	/// clear the cache
	pub fn clear_cache(&mut self) {
		self.cache.clear();
	}
}

#[derive(Deserialize, Debug)]
struct Frontmatter {
	pub template: Option<FrontmatterTemplate>,
	pub props: Option<liquid::Object>,
}

#[derive(Deserialize, Debug)]
struct FrontmatterTemplate {
	pub path: Option<PathBuf>,
	#[serde(default)]
	pub local: bool,
}

fn with_added_extension_but_stable(path: &Path, extension: impl AsRef<OsStr>) -> PathBuf {
	let mut new = path.extension().unwrap_or_default().to_os_string();
	if path.extension().is_some() {
		new.push(".");
	}
	new.push(extension);
	path.with_extension(new)
}

/// the default globals for [`create_templated`], which passes `props` as the global `props` and `body` as the global `body`
pub fn default_globals(_: PathBuf, props: Option<Object>, body: String) -> Object {
	object!({
		"body": body,
		"props": props.unwrap_or_default(),
	})
}

/// wraps a language parser so it can be easily shared between multiple rules
pub fn shared_lang(
	lang: impl for<'a> FnMut(&'a str) -> Result<(String, String), ErrorKind>,
) -> impl for<'a> FnMut(&'a str) -> Result<(String, String), ErrorKind> + Clone {
	let lang = Rc::new(RefCell::new(lang));

	move |src| lang.borrow_mut()(src)
}

/// compile liquid templates + a source language
///
/// - `default_template` - the template to use when not overridden by a given source file
/// - `liquid` - a shared cell of the liquid parser instance
/// - `globals` - the globals to use in templating
///   - takes the source file path, props from frontmatter, and compiled content from `lang`
///   - returns the globals
///   
///   if you don't have a purpose for this, you should probably set it to [`default_globals`]
/// - `lang` - the source language to parse
///   - takes the content of the source file
///   - returns (frontmatter (unparsed), content)
pub fn create_templated(
	default_template: PathBuf,
	liquid: Rc<RefCell<Liquid>>,
	mut globals: impl for<'a> FnMut(PathBuf, Option<Object>, String) -> Object,
	mut lang: impl for<'a> FnMut(&'a str) -> Result<(String, String), ErrorKind>,
) -> impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind> {
	move |src: PathBuf, dst, _| {
		let content = fs::read_to_string(&src)?;

		let (frontmatter, body) = lang(&content)?;

		let frontmatter =
			from_str::<Frontmatter>(&frontmatter).map_err(LiquidErrorKind::FrontmatterParsing)?;

		let template = if let Some(template) = frontmatter.template {
			with_added_extension_but_stable(
				&if template.local {
					if let Some(path) = template.path {
						if path.is_absolute() {
							return Err(LiquidErrorKind::FrontmatterAbsoluteLocalPath(path).into());
						}

						src.parent().unwrap().join(path)
					} else {
						src.with_extension("")
					}
				} else {
					if let Some(path) = template.path {
						path
					} else {
						default_template.clone()
					}
				},
				"liquid",
			)
		} else {
			default_template.clone()
		};

		liquid
			.borrow_mut()
			.parse(&template)?
			.render_to(
				&mut OpenOptions::new()
					.create(true)
					.write(true)
					.append(false)
					.read(false)
					.open(&dst)?,
				&globals(src, frontmatter.props, body),
			)
			.map_err(|err| LiquidErrorKind::Liquid(err, Some(dst)))?;

		Ok(())
	}
}

/// compile liquid templates standalone
///
/// - `liquid` - a shared cell of the liquid parser instance
/// - `globals` - the globals to use in templating
///   - takes the source file path
///   - returns the globals
///   
///   if you don't have a purpose for this, you should probably return [`Default::default`]
pub fn create_standalone(
	liquid: Rc<RefCell<Liquid>>,
	mut globals: impl for<'a> FnMut(PathBuf) -> Object,
) -> impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind> {
	move |src: PathBuf, dst, _| {
		liquid
			.borrow_mut()
			.parse(&src)?
			.render_to(
				&mut OpenOptions::new()
					.create(true)
					.write(true)
					.append(false)
					.read(false)
					.open(&dst)?,
				&globals(src),
			)
			.map_err(|err| LiquidErrorKind::Liquid(err, Some(dst)))?;

		Ok(())
	}
}

/// renamed to [`create_templated`]
#[deprecated = "renamed to create_templated"]
pub fn create(
	default_template: PathBuf,
	liquid: Rc<RefCell<Liquid>>,
	globals: impl for<'a> FnMut(PathBuf, Option<Object>, String) -> Object,
	lang: impl for<'a> FnMut(&'a str) -> Result<(String, String), ErrorKind>,
) -> Result<impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind>, ErrorKind> {
	Ok(create_templated(default_template, liquid, globals, lang))
}

/// an error while using liquid templating
#[derive(::thiserror::Error, Debug)]
pub enum LiquidErrorKind {
	/// template parsing failed
	#[error("template parsing failed")]
	Liquid(#[source] ::liquid::Error, Option<PathBuf>),

	/// frontmatter parsing failed
	#[error("frontmatter parsing failed")]
	FrontmatterParsing(#[from] ::toml::de::Error),

	/// frontmatter asks for a local template, but provides an absolute path
	#[error("frontmatter asks for a local template, but provides an absolute path")]
	FrontmatterAbsoluteLocalPath(PathBuf),

	/// markdoll error
	///
	/// requires `liquid-markdoll` feature
	#[cfg(feature = "liquid-markdoll")]
	#[error("markdoll failed")]
	Markdoll(Vec<::markdoll::diagnostics::Diagnostic>),
}
