//! [markdoll](https://codeberg.org/0x57e11a/markdoll) support
//!
//! requires `lang-markdoll` feature

use {
	crate::{lang::LangErrorKind, ErrorKind},
	::core::fmt::Debug,
	::markdoll::{diagnostics::DiagnosticKind, spanner::Spanner, MarkDoll, MarkDollSrc},
	::miette::{Diagnostic, Report, Severity},
	::std::{path::Path, sync::Arc},
	::tracing::trace_span,
};

pub extern crate hashbrown;
pub extern crate markdoll;

/// language support for markdoll
pub fn create<Ctx, To: Debug + Into<String> + 'static>(
	mut doll: MarkDoll<Ctx>,
	to: impl Fn(&Path) -> To,
	ctx: impl Fn(&Path) -> Ctx,
) -> impl for<'a> FnMut(&'a str, &'a Path) -> Result<(String, String), ErrorKind> {
	fn diag_beh(diagnostics: Vec<DiagnosticKind>, spanner: &Arc<Spanner<MarkDollSrc>>) -> usize {
		let mut n = 0;
		for diagnostic in diagnostics {
			if let Some(Severity::Error) | None = diagnostic.severity() {
				n += 1;
			}
			eprintln!(
				"{:?}",
				Report::from(diagnostic).with_source_code(spanner.clone())
			);
		}
		n
	}
	move |src, path| {
		let _span = trace_span!("compile markdoll").entered();

		let (ok, mut diagnostics, frontmatter, mut ast) = doll.parse_document(
			path.to_str()
				.ok_or(ErrorKind::NonUTF8PathCharacters)?
				.to_string(),
			src.to_string(),
			None,
		);

		if ok {
			let mut to = to(path);
			let mut ctx = ctx(path);

			let (emit_ok, mut emit_diagnostics) = doll.emit(&mut ast, &mut to, &mut ctx);

			diagnostics.append(&mut emit_diagnostics);

			let n = diag_beh(diagnostics, &doll.finish());
			if emit_ok && n == 0 {
				Ok((frontmatter.unwrap_or_default(), to.into()))
			} else {
				Err(ErrorKind::Lang(LangErrorKind::Markdoll(n)))
			}
		} else {
			let n = diag_beh(diagnostics, &doll.finish());

			Err(ErrorKind::Lang(LangErrorKind::Markdoll(n)))
		}
	}
}
