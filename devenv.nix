{ pkgs, lib, config, inputs, ... }: {
	packages = with pkgs; [ git ];

	languages.rust.enable = true;
	languages.rust.channel = "nightly";
}
