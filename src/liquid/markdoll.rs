use {
	crate::{liquid::LiquidErrorKind, ErrorKind},
	::hashbrown::HashMap,
	::markdoll::{diagnostics::render, emit::HtmlEmit, MarkDoll},
};

pub extern crate ariadne;
pub extern crate hashbrown;
pub extern crate markdoll;

pub fn create(
	mut doll: MarkDoll,
	code_block_format: HashMap<&'static str, fn(doll: &mut MarkDoll, emit: &mut HtmlEmit, &str)>,
) -> impl for<'a> FnMut(&'a str) -> Result<(String, String), ErrorKind> {
	move |src| {
		let mut cache = ariadne::Source::from(src);

		if let Ok((frontmatter, mut ast)) = doll.parse_document(src) {
			let mut to = HtmlEmit {
				write: String::new(),
				section_level: 0,
				code_block_format: code_block_format.clone(),
			};

			if doll.emit(&mut ast, &mut to) {
				for report in render(&doll.finish()) {
					report.eprint(&mut cache)?;
				}

				return Ok((frontmatter.unwrap_or_default(), to.write));
			}
		}

		let mut diagnostics = doll.finish();
		diagnostics.retain(|diagnostic| {
			if diagnostic.err {
				true
			} else {
				render(core::slice::from_ref(diagnostic))[0]
					.eprint(&mut cache)
					.unwrap();
				false
			}
		});

		Err(ErrorKind::LiquidIntegration(LiquidErrorKind::Markdoll(
			diagnostics,
		)))
	}
}
