use {
	::anyhow::bail,
	::core::cell::RefCell,
	::dollgen::{
		lang::markdoll::{
			hashbrown::HashMap,
			markdoll::{
				emit::{html::HtmlEmit, BuiltInEmitters},
				MarkDoll,
			},
		},
		liquid::{liquid::ParserBuilder, Liquid},
		scss,
		Pattern,
		Rule,
	},
	::minijinja::Environment,
	::std::{env, fs, path::Path, rc::Rc},
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
		doll.add_tags(markdoll::ext::common::tags());
		doll.add_tags(markdoll::ext::formatting::tags());
		doll.add_tags(markdoll::ext::code::tags());
		doll.add_tags(markdoll::ext::links::tags());
		doll.add_tags(markdoll::ext::table::tags());
		doll.builtin_emitters.put(HtmlEmit::DEFAULT_EMITTERS);

		dollgen::lang::shared_lang(dollgen::lang::markdoll::create(doll, |_| {
			HtmlEmit::default()
		}))
	};

	let liquid = Liquid::new(ParserBuilder::new().stdlib().build().unwrap());

	let minijinja = Rc::new(RefCell::new({
		let mut env = Environment::new();
		env.set_loader(|name| Ok(dbg!(fs::read_to_string(name)).ok()));
		env.add_function(::minijinja::functions::, f);
		env
	}));

	if let Err(err) = dollgen::run(&mut [
		// liquid
		Rule {
			include: &[Pattern::new("src/(**)/(*).useliquid.doll")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "dist/{0}/{1}.html",
			plan: &mut dollgen::liquid::create_templated(
				Path::new("templates/page.liquid").to_path_buf(),
				liquid.clone(),
				dollgen::liquid::default_globals,
				doll_lang.clone(),
			),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).page.liquid")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "dist/{0}/{1}.html",
			plan: &mut dollgen::liquid::create_standalone(liquid.clone(), |_| Default::default()),
		},
		// jinja
		Rule {
			include: &[Pattern::new("src/(**)/(*).useminijinja.doll")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "dist/{0}/{1}.html",
			plan: &mut dollgen::minijinja::create_templated(
				Path::new("templates/awa.jinja").to_path_buf(),
				minijinja.clone(),
				dollgen::minijinja::default_globals,
				doll_lang.clone(),
			),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).page.jinja")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "dist/{0}/{1}.html",
			plan: &mut dollgen::minijinja::create_standalone(minijinja.clone(), |_| {
				Default::default()
			}),
		},
		// other
		Rule {
			include: &[Pattern::new("src/(**)/(*).html")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "dist/{0}/{1}.html",
			plan: &mut dollgen::copy,
		},
		Rule {
			include: &[Pattern::new("src/(**)/.build-wasm")?],
			exclude: &[],
			dst: "dist/{0}.wasm",
			plan: &mut dollgen::wasm::create_both(true, "dist/{0}.js", "gen_types/{0}.d.ts"),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).scss")?],
			exclude: &[],
			dst: "dist/{0}/{1}.css",
			plan: &mut scss::create(
				&scss::grass::Options::default().style(scss::grass::OutputStyle::Compressed),
			),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).asset.(*)")?],
			exclude: &[],
			dst: "dist/{0}/{1}.{2}",
			plan: &mut dollgen::copy,
		},
	]) {
		println!("{err:#?}\n{err}");
	}

	Ok(())
}
