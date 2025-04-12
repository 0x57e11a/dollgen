use {
	::anyhow::bail,
	::core::cell::RefCell,
	::dollgen::{
		lang::markdoll::markdoll::{emit::html::HtmlEmit, MarkDoll},
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

	if Path::new("deploy").is_dir() {
		fs::remove_dir_all("deploy")?;
	}

	env::set_current_dir("examples")?;

	let doll_lang = {
		let mut doll = MarkDoll::new();
		doll.add_tags(::markdoll::ext::all_tags());
		doll.builtin_emitters.put(HtmlEmit::default_emitters());

		::dollgen::lang::shared_lang(::dollgen::lang::markdoll::create(
			doll,
			|_| HtmlEmit::default(),
			|_| (),
		))
	};

	let liquid = Liquid::new(ParserBuilder::new().stdlib().build().unwrap());

	let minijinja = Rc::new(RefCell::new({
		let mut env = Environment::new();
		env.set_loader(|name| Ok(fs::read_to_string(name).ok()));
		env
	}));

	if let Err(err) = ::dollgen::run(&mut [
		// liquid
		Rule {
			include: &[Pattern::new("src/(**)/(*).useliquid.doll")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "deploy/{0}/{1}.html",
			plan: &mut ::dollgen::liquid::create_templated(
				Path::new("templates/page.liquid").to_path_buf(),
				liquid.clone(),
				::dollgen::liquid::default_globals,
				doll_lang.clone(),
			),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).page.liquid")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "deploy/{0}/{1}.html",
			plan: &mut ::dollgen::liquid::create_standalone(liquid.clone(), |_| Default::default()),
		},
		// jinja
		Rule {
			include: &[Pattern::new("src/(**)/(*).usejinja.doll")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "deploy/{0}/{1}.html",
			plan: &mut ::dollgen::minijinja::create_templated(
				Path::new("templates/awa.jinja").to_path_buf(),
				minijinja.clone(),
				::dollgen::minijinja::default_globals,
				doll_lang.clone(),
			),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).page.jinja")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "deploy/{0}/{1}.html",
			plan: &mut ::dollgen::minijinja::create_standalone(minijinja.clone(), |_| {
				Default::default()
			}),
		},
		// other
		Rule {
			include: &[Pattern::new("src/(**)/(*).html")?],
			exclude: &[Pattern::new("**/*.draft.*")?],
			dst: "deploy/{0}/{1}.html",
			plan: &mut ::dollgen::copy,
		},
		Rule {
			include: &[Pattern::new("src/(**)/.build-wasm")?],
			exclude: &[],
			dst: "deploy/{0}.wasm",
			plan: &mut ::dollgen::wasm::create_both(true, "deploy/{0}.js", "gen_types/{0}.d.ts"),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).scss")?],
			exclude: &[],
			dst: "deploy/{0}/{1}.css",
			plan: &mut scss::create(
				&scss::grass::Options::default().style(scss::grass::OutputStyle::Compressed),
			),
		},
		Rule {
			include: &[Pattern::new("src/(**)/(*).asset.(*)")?],
			exclude: &[],
			dst: "deploy/{0}/{1}.{2}",
			plan: &mut ::dollgen::copy,
		},
	]) {
		println!("{err:#?}\n{err}");
	}

	Ok(())
}
