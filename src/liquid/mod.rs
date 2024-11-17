use {
	crate::ErrorKind,
	::core::cell::RefCell,
	::hashbrown::{hash_map::Entry, HashMap},
	::liquid::{object, Parser, ParserBuilder, Template},
	::serde::Deserialize,
	::std::{
		fs::{self, OpenOptions},
		path::{Path, PathBuf},
		rc::Rc,
	},
	::toml::from_str,
};

#[cfg(feature = "liquid-markdoll")]
pub mod markdoll;

pub extern crate liquid;

pub struct Liquid {
	pub parser: Parser,
	cache: HashMap<PathBuf, Template>,
}

impl Liquid {
	pub fn new(pb: ParserBuilder) -> Result<Rc<RefCell<Self>>, ErrorKind> {
		Ok(Rc::new(RefCell::new(Self {
			parser: pb
				.build()
				.map_err(|err| LiquidErrorKind::Liquid(err, None))?,
			cache: HashMap::new(),
		})))
	}

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

	pub fn clear_cache(&mut self) {
		self.cache.clear();
	}
}

pub fn create_template(
	template: PathBuf,
	liquid: Rc<RefCell<Liquid>>,
	mut lang: impl for<'a> FnMut(&'a str) -> Result<(String, String), ErrorKind>,
) -> Result<impl FnMut(PathBuf, PathBuf, Vec<String>) -> Result<(), ErrorKind>, ErrorKind> {
	Ok(move |src, dst, _| {
		#[derive(Deserialize, Debug)]
		struct Frontmatter {
			pub template: Option<PathBuf>,
			pub props: Option<liquid::Object>,
		}

		let content = fs::read_to_string(&src)?;

		let (frontmatter, body) = lang(&content)?;

		let frontmatter =
			from_str::<Frontmatter>(&frontmatter).map_err(LiquidErrorKind::FrontmatterParsing)?;

		liquid
			.borrow_mut()
			.parse(frontmatter.template.as_deref().unwrap_or(&*template))?
			.render_to(
				&mut OpenOptions::new()
					.create(true)
					.write(true)
					.append(false)
					.read(false)
					.open(&dst)?,
				&object!({
					"content": body,
					"props": frontmatter.props,
				}),
			)
			.map_err(|err| LiquidErrorKind::Liquid(err, Some(dst)))?;

		Ok(())
	})
}

pub fn shared_lang(
	lang: impl for<'a> FnMut(&'a str) -> Result<(String, String), ErrorKind>,
) -> impl for<'a> FnMut(&'a str) -> Result<(String, String), ErrorKind> + Clone {
	let lang = Rc::new(RefCell::new(lang));

	move |src| lang.borrow_mut()(src)
}

#[derive(::thiserror::Error, Debug)]
pub enum LiquidErrorKind {
	#[error("liquid parsing failed")]
	Liquid(::liquid::Error, Option<PathBuf>),

	#[error("frontmatter error")]
	FrontmatterParsing(#[from] ::toml::de::Error),

	#[cfg(feature = "liquid-markdoll")]
	#[error("markdoll failed")]
	Markdoll(Vec<::markdoll::diagnostics::Diagnostic>),

	#[error(transparent)]
	Other(::anyhow::Error),
}