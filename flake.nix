{
  description = "A Nix-flake-based Rust development environment with mq-cli";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs:
    let
      supportedSystems =
        [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachSupportedSystem = f:
        inputs.nixpkgs.lib.genAttrs supportedSystems (system:
          f {
            pkgs = import inputs.nixpkgs {
              inherit system;
              overlays = [
                inputs.rust-overlay.overlays.default
                inputs.self.overlays.default
              ];
            };
          });
    in {
      overlays.default = final: prev: {
        rustToolchain = let rust = prev.rust-bin;
        in if builtins.pathExists ./rust-toolchain.toml then
          rust.fromRustupToolchainFile ./rust-toolchain.toml
        else if builtins.pathExists ./rust-toolchain then
          rust.fromRustupToolchainFile ./rust-toolchain
        else
          rust.stable.latest.default.override {
            extensions = [ "rust-src" "rustfmt" ];
          };
      };

      devShells = forEachSupportedSystem ({ pkgs }: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            openssl
            pkg-config
            cargo-deny
            cargo-edit
            cargo-watch
            rust-analyzer
          ];

          env = {
            RUST_SRC_PATH =
              "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
          };
        };
      });

      packages = forEachSupportedSystem ({ pkgs }: {
        mq-cli = pkgs.rustPlatform.buildRustPackage rec {
          pname = "mq-cli";
          version = "unstable";

          src = pkgs.fetchFromGitHub {
            owner = "harehare";
            repo = "mq";
            rev = "main";
            sha256 = "sha256-4cQjQnPNgPKtnyVR46Hu9G5sn5QbmQqFhHK4DZfHlKo=";
          };

          cargoLock = {
            lockFile = null;
            lockFileContents = builtins.readFile ./Cargo.lock;
          };

          meta = with pkgs.lib; {
            description = "Markdown processor with jq-like syntax";
            homepage = "https://github.com/harehare/mq";
            license = licenses.mit;
            maintainers = with maintainers; [ YOUR_GITHUB_USERNAME ];
            platforms = platforms.linux;
          };
        };
      });
    };
}

