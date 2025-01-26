{ pkgs, lib, config, inputs, ... }: {
	packages = with pkgs; [ git miniserve ];

	languages.rust = {
		enable = true;
		channel = "nightly";
		targets = [
			"wasm32-unknown-unknown"
		];
	};
}
