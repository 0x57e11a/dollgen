//! compile liquid templates, based on input languages
//!
//! languages parse their source code and may provide a frontmatter string, which is parsed as TOML:
//!
//! - `template` (optional)
//!   - if `template.local` is true:
//!     - if `template.path` is defined, that template is used, and the path is assumed to be relative to the directory containing the source file
//!     - if `template.path` is not defined, it uses the template with the same name as the source file (ex: `page.doll` will use `page.liquid` in the same directory)
//!   - if `template.local` is false or not specified:
//!     - if `template.path` is defined, that template is used, and the path is assumed to be relative to the root of the build
//!     - if `template.path` is not defined, the default template is used
//! - `props` (optional)
//!   - values are fed into the liquid template
//!
//! requires `liquid` feature

use {
	crate::{util::with_added_extension_but_stable, ErrorKind, PlannedTransformation},
	::core::cell::RefCell,
	::hashbrown::{hash_map::EntryRef, HashMap},
	::liquid::{object, Object, Parser, Template},
	::serde::Deserialize,
	::std::{
		fs::{self, OpenOptions},
		path::{Path, PathBuf},
		rc::Rc,
	},
	::toml::from_str,
	::tracing::{instrument, trace_span, Level},
};

pub extern crate liquid;

/// parses and caches liquid templates
///
/// ensure to [`clear_cache`](Liquid::clear_cache) in case templates change
pub struct Liquid {
	/// the parser
	pub parser: Parser,
	cache: HashMap<PathBuf, Rc<Template>>,
}

impl Liquid {
	/// create from a liquid parser builder
	#[must_use]
	pub fn new(parser: Parser) -> Rc<RefCell<Self>> {
		Rc::new(RefCell::new(Self {
			parser,
			cache: HashMap::new(),
		}))
	}

	/// parse a template file or retrieve from cache
	pub fn parse(&mut self, path: &Path) -> Result<Rc<Template>, ErrorKind> {
		Ok(match self.cache.entry_ref(path) {
			EntryRef::Occupied(entry) => entry.into_mut(),
			EntryRef::Vacant(entry) => {
				entry.insert(Rc::new(self.parser.parse_file(path).map_err(|err| {
					let source_code = match fs::read_to_string(path) {
						Ok(src) => src,
						Err(err) => return ErrorKind::Io(err),
					};

					ErrorKind::LiquidIntegration(LiquidErrorKind::LiquidParsing(
						err,
						path.to_path_buf(),
						source_code,
					))
				})?))
			}
		}
		.clone())
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

/// the default globals for [`create_templated`], which passes `props` as the global `props` and `body` as the global `body`
#[must_use]
pub fn default_globals(_: PathBuf, props: Option<Object>, body: String) -> Object {
	object!({
		"body": body,
		"props": props.unwrap_or_default(),
	})
}

/// a plan to render a liquid template
#[::tyfling::debug(.globals)]
pub struct LiquidPlan {
	/// the template
	pub template: Rc<Template>,
	/// the globals
	pub globals: Object,
}

impl PlannedTransformation for LiquidPlan {
	#[instrument(, name = "render liquid template", level = Level::DEBUG)]
	fn execute(self: Box<Self>, dst: PathBuf) -> Result<(), ErrorKind> {
		self.template
			.render_to(
				&mut OpenOptions::new()
					.create(true)
					.write(true)
					.truncate(true)
					.append(false)
					.read(false)
					.open(&dst)?,
				&self.globals,
			)
			.map_err(|err| ErrorKind::LiquidIntegration(LiquidErrorKind::LiquidRendering(err, dst)))
	}
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
	mut lang: impl for<'a> FnMut(&'a str, &'a Path) -> Result<(String, String), ErrorKind>,
) -> impl FnMut(PathBuf, Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	move |src: PathBuf, _| {
		let _span = trace_span!("templated liquid", ?default_template).entered();

		let content = fs::read_to_string(&src)?;

		let (frontmatter, body) = lang(&content, &src)?;

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
				} else if let Some(path) = template.path {
					path
				} else {
					default_template.clone()
				},
				"liquid",
			)
		} else {
			default_template.clone()
		};

		Ok(Box::new(LiquidPlan {
			template: liquid.borrow_mut().parse(&template)?,
			globals: globals(src, frontmatter.props, body),
		}))
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
) -> impl FnMut(PathBuf, Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	move |src: PathBuf, _| {
		let _span = trace_span!("standalone liquid").entered();

		Ok(Box::new(LiquidPlan {
			template: liquid.borrow_mut().parse(&src)?,
			globals: globals(src),
		}))
	}
}

/// an error while using liquid templating
#[derive(::thiserror::Error, ::miette::Diagnostic, Debug)]
pub enum LiquidErrorKind {
	/// template parsing failed
	#[error("template parsing failed for {}", .1.to_str().unwrap())]
	#[diagnostic(code(dollgen::liquid::template_parse_failed))]
	LiquidParsing(#[source] ::liquid::Error, PathBuf, #[source_code] String),

	/// template rendering failed
	#[error("template rendering failed for {}", .1.to_str().unwrap())]
	#[diagnostic(code(dollgen::liquid::template_parse_failed))]
	LiquidRendering(#[source] ::liquid::Error, PathBuf),

	/// frontmatter parsing failed
	#[error("frontmatter parsing failed")]
	#[diagnostic(code(dollgen::liquid::frontmatter_parse_failed))]
	FrontmatterParsing(#[source] ::toml::de::Error),

	/// frontmatter requests a local template, but provides an absolute path
	#[error("frontmatter requests a local template, but provides an absolute path")]
	#[diagnostic(
		code(dollgen::liquid::frontmatter_absolute_local_path),
		help("either change to a relative path or remove the local attribute")
	)]
	FrontmatterAbsoluteLocalPath(PathBuf),
}
