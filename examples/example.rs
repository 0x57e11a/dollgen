use {
	::anyhow::bail,
	::dollgen::{
		liquid::{
			liquid::ParserBuilder,
			markdoll::{
				hashbrown::HashMap,
				markdoll::{
					emit::{BuiltInEmitters, HtmlEmit},
					MarkDoll,
				},
			},
			Liquid,
		},
		scss,
		Pattern,
		Rule,
	},
	::std::{env, fs, path::Path},
};

fn main() -> Result<(), anyhow::Error> {
	if !Path::new("src").is_dir() {
		bail!("`src` does not exist");
	}

	if Path::new("dist").is_dir() {
		fs::remove_dir_all("dist")?;
	}

	env::set_current_dir("examples")?;

	let doll_lang = {
		let mut doll = MarkDoll::new();
		doll.ext_system.add_tags(markdoll::ext::common::tags());
		doll.ext_system.add_tags(markdoll::ext::formatting::tags());
		doll.ext_system.add_tags(markdoll::ext::code::tags());
		doll.ext_system.add_tags(markdoll::ext::links::tags());
		doll.ext_system.add_tags(markdoll::ext::table::tags());
		doll.set_emitters(BuiltInEmitters::<HtmlEmit>::default());

		let code_block_format = HashMap::new();

		dollgen::liquid::shared_lang(dollgen::liquid::markdoll::create(doll, code_block_format))
	};

	let liquid = Liquid::new(ParserBuilder::new().stdlib())?;

	if let Err(err) = dollgen::run(&mut [
		Rule {
			include: &[Pattern::new("src/(**)/(*).doll")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "dist/{0}/{1}.html",
			transformer: &mut dollgen::liquid::create(
				Path::new("templates/page.liquid").to_path_buf(),
				liquid.clone(),
				dollgen::liquid::default_globals,
				doll_lang.clone(),
			)?,
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).html")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "dist/{0}/{1}.html",
			transformer: &mut dollgen::copy,
		},
		Rule {
			include: &[Pattern::new("src/(**)/.build-wasm")?],
			exclude: &[],
			dst: "dist/{0}.wasm",
			transformer: &mut dollgen::wasm::create_both(true, "dist/{0}.js", "gen_types/{0}.d.ts"),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).scss")?],
			exclude: &[],
			dst: "dist/{0}/{1}.css",
			transformer: &mut scss::create(
				scss::grass::Options::default().style(scss::grass::OutputStyle::Compressed),
			),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).asset.(*)")?],
			exclude: &[],
			dst: "dist/{0}/{1}.{2}",
			transformer: &mut dollgen::copy,
		},
	]) {
		println!("{err:#?}\n{err}");
	}

	Ok(())
}
