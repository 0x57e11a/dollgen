use {
	crate::{util::with_added_extension_but_stable, ErrorKind, PlannedTransformation},
	::core::cell::RefCell,
	::hashbrown::{hash_map::EntryRef, HashMap},
	::minijinja::{context, Environment, Template, Value},
	::serde::{Deserialize, Serialize},
	::std::{
		ffi::OsStr,
		fs::{self, OpenOptions},
		path::{Path, PathBuf},
		rc::Rc,
	},
	::toml::from_str,
	::tracing::{instrument, trace_span, Level},
};

pub extern crate minijinja;

#[derive(Debug, Deserialize)]
struct Frontmatter {
	pub template: Option<FrontmatterTemplate>,
	pub props: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct FrontmatterTemplate {
	pub path: Option<PathBuf>,
	#[serde(default)]
	pub local: bool,
}

/// the default globals for [`create_templated`], which passes `props` as the global `props` and `body` as the global `body`
pub fn default_globals(_: PathBuf, props: Option<Value>, body: String) -> Value {
	context! {
		props => props.unwrap_or_default(),
		body => body,
	}
}

#[::tyfling::debug(.globals)]
pub struct MinijinjaPlan {
	pub env: Rc<RefCell<Environment<'static>>>,
	pub template: String,
	pub globals: Value,
}

impl PlannedTransformation for MinijinjaPlan {
	#[instrument(, name = "render liquid template", level = Level::DEBUG)]
	fn execute(self: Box<Self>, dst: PathBuf) -> Result<(), ErrorKind> {
		self.env
			.borrow()
			.get_template(&self.template)
			.map_err(|err| {
				ErrorKind::MinijinjaIntegration(MinijinjaErrorKind::MinijinjaRendering(
					err,
					dst.clone(),
				))
			})?
			.render_to_write(
				&self.globals,
				&mut OpenOptions::new()
					.create(true)
					.write(true)
					.truncate(true)
					.append(false)
					.read(false)
					.open(&dst)?,
			)
			.map_err(|err| {
				ErrorKind::MinijinjaIntegration(MinijinjaErrorKind::MinijinjaRendering(err, dst))
			})?;

		Ok(())
	}
}

pub fn create_templated(
	default_template: PathBuf,
	env: Rc<RefCell<Environment<'static>>>,
	mut globals: impl for<'a> FnMut(PathBuf, Option<Value>, String) -> Value,
	mut lang: impl for<'a> FnMut(&'a str, &'a Path) -> Result<(String, String), ErrorKind>,
) -> impl FnMut(PathBuf, Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	move |src: PathBuf, _| {
		let _span = trace_span!("templated minijinja", ?default_template).entered();

		let content = fs::read_to_string(&src)?;

		let (frontmatter, body) = lang(&content, &src)?;

		let frontmatter = from_str::<Frontmatter>(&frontmatter)
			.map_err(MinijinjaErrorKind::FrontmatterParsing)?;

		let template = if let Some(template) = frontmatter.template {
			with_added_extension_but_stable(
				&if template.local {
					if let Some(path) = template.path {
						if path.is_absolute() {
							return Err(
								MinijinjaErrorKind::FrontmatterAbsoluteLocalPath(path).into()
							);
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
				"jinja",
			)
		} else {
			default_template.clone()
		};

		Ok(Box::new(MinijinjaPlan {
			env: env.clone(),
			template: template.to_str().unwrap().to_string(),
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
	env: Rc<RefCell<Environment<'static>>>,
	mut globals: impl for<'a> FnMut(PathBuf) -> Value,
) -> impl FnMut(PathBuf, Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> {
	move |src: PathBuf, _| {
		let _span = trace_span!("standalone minijinja").entered();

		Ok(Box::new(MinijinjaPlan {
			env: env.clone(),
			template: src.to_str().unwrap().to_string(),
			globals: globals(src),
		}))
	}
}

/// an error while using liquid templating
#[derive(::thiserror::Error, ::miette::Diagnostic, Debug)]
pub enum MinijinjaErrorKind {
	/// template parsing failed
	#[error("template parsing failed for {}", .1.to_str().unwrap())]
	#[diagnostic(code(dollgen::minijinja::template_parse_failed))]
	MinijinjaParsing(#[source] ::minijinja::Error, PathBuf, #[source_code] String),

	/// template rendering failed
	#[error("template rendering failed for {}", .1.to_str().unwrap())]
	#[diagnostic(code(dollgen::minijinja::template_parse_failed))]
	MinijinjaRendering(#[source] ::minijinja::Error, PathBuf),

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
