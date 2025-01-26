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
pub fn create<To: Debug + Into<String> + 'static>(
	mut doll: MarkDoll,
	to: impl Fn(&Path) -> To,
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
		);

		if ok {
			let mut to = to(path);

			let (emit_ok, mut emit_diagnostics) = doll.emit(&mut ast, &mut to);

			diagnostics.append(&mut emit_diagnostics);

			let n = diag_beh(diagnostics, &doll.finish());
			if emit_ok {
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
