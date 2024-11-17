{ pkgs, lib, config, inputs, ... }: {
	packages = with pkgs; [ git ];

	languages.rust = {
		enable = true;
		channel = "nightly";
		targets = [
			"wasm32-unknown-unknown"
		];
	};
}
