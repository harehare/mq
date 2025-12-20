{
  description = "A Nix-flake-based Rust development environment with mq";

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
            cargo-codspeed
            cargo-deny
            cargo-edit
            cargo-llvm-cov
            cargo-udeps
            cargo-nextest
            cargo-watch
            just
            maturin
            nodejs_24
            openssl
            pkg-config
            python314
            rust-analyzer
            rustToolchain
            twine
            wasm-pack
            ];

          env = {
            RUST_SRC_PATH =
              "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
          };
        };
      });

      packages = forEachSupportedSystem ({ pkgs }: {
        mq = pkgs.rustPlatform.buildRustPackage {
          pname = "mq";
          version = "unstable";

          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          meta = with pkgs.lib; {
            description = "jq-like command-line tool for markdown processing";
            homepage = "https://github.com/harehare/mq";
            license = licenses.mit;
            maintainers = with maintainers; [ harehare ];
            platforms = platforms.unix;
          };
        };
      });
    };
}
