{ inputs, lib, ... }:
{
  perSystem =
    { pkgs, system, ... }:
    {
      _module.args.pkgs = import inputs.nixpkgs {
        inherit system;
        overlays = [ inputs.rust-overlay.overlays.default ];
        config.allowUnfree = true; # Allow unfree packages like claude-code
      };

      legacyPackages =
        let
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-analyzer"
              "rustfmt"
              "clippy"
              "rust-src"
            ];
            targets = [
              # Linux
              "x86_64-unknown-linux-gnu"
              "aarch64-unknown-linux-gnu"
              # macOS
              "x86_64-apple-darwin"
              "aarch64-apple-darwin"
              # Windows (GNU)
              "x86_64-pc-windows-gnu"
              "aarch64-pc-windows-gnullvm"
            ];
          };
        in
        {
          inherit rustToolchain;
        };
    };
}
