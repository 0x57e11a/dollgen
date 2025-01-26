use {
	crate::{ErrorKind, PlannedTransformation},
	::grass::{from_path, Options},
	::miette::LabeledSpan,
	::std::{path::PathBuf, sync::Arc},
	::tracing::debug_span,
};

pub extern crate grass;

/// compiles scss/sass
///
/// - `options` - the options to compile with
pub fn create<'a>(
	options: &'a Options<'a>,
) -> impl FnMut(PathBuf, Vec<String>) -> Result<Box<dyn PlannedTransformation>, ErrorKind> + 'a {
	move |src, _| {
		let _span = debug_span!("compile scss", ?options).entered();

		Ok(Box::new(from_path(src, &options).map_err(|err| {
			match err.kind() {
				::grass::ErrorKind::ParseError { message, loc, .. } => ErrorKind::SCSSIntegration {
					src: ::miette::NamedSource::new(loc.file.name(), loc.file.source().to_string())
						.with_language("scss"),
					span: [LabeledSpan::new_primary_with_span(Some(message), {
						let filestart = loc.file.span.low();
						(
							(loc.file.line_span(loc.begin.line).low() - filestart) as usize
								+ loc.begin.column,
							(loc.file.line_span(loc.end.line).low() - filestart) as usize
								+ loc.begin.column,
						)
					})],
				},
				::grass::ErrorKind::IoError(io) => Arc::into_inner(io).unwrap().into(),
				::grass::ErrorKind::FromUtf8Error(_) => ErrorKind::NonUTF8Characters,
				_ => todo!(),
			}
		})?))
	}
}
