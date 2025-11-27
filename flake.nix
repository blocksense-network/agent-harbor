{
  description = "Agent Harbor";

  nixConfig = {
    extra-substituters = [
      "https://agent-harbor.cachix.org"
      "https://mcl-public-cache.cachix.org"
    ];
    extra-trusted-public-keys = [
      "agent-harbor.cachix.org-1:2x123W9OUoHUzXoSvPv2CRXPo7rjLKAOd6/MkaHFNRA="
      "mcl-public-cache.cachix.org-1:OcUzMeoSAwNEd3YCaEbNjLV5/Gd+U5VFxdN2WGHfpCI="
    ];
  };

  inputs = {
    nixos-modules.url = "github:metacraft-labs/nixos-modules";

    nixpkgs.follows = "nixos-modules/nixpkgs-unstable";
    # TODO: Replace with fenix
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks.follows = "nixos-modules/git-hooks-nix";
    flake-parts.follows = "nixos-modules/flake-parts";

    nix-ai-tools = {
      url = "github:numtide/nix-ai-tools";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.treefmt-nix.follows = "nixos-modules/treefmt-nix";
    };
    codex = {
      url = "git+file:./vendor/codex";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "nixos-modules/flake-utils";
      inputs.rust-overlay.follows = "rust-overlay";
    };
    sosumi-docs-downloader = {
      url = "git+https://github.com/blocksense-network/sosumi-docs-downloader.git";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.git-hooks.follows = "git-hooks";
      inputs.rust-overlay.follows = "rust-overlay";
    };
    pjdfstest-src = {
      url = "https://github.com/pjd/pjdfstest/archive/master.tar.gz";
      flake = false;
    };
    codetracer-python-recorder = {
      url = "github:metacraft-labs/codetracer-python-recorder?dir=nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      rust-overlay,
      git-hooks,
      nix-ai-tools,
      codex,
      sosumi-docs-downloader,
      pjdfstest-src,
      codetracer-python-recorder,
      flake-parts,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } (
      { config, lib, ... }:
      rec {
        systems = [
          "x86_64-linux"
          "aarch64-linux"
          "x86_64-darwin"
          "aarch64-darwin"
        ];

        imports = [
          ./nix/pre-commit.nix
          ./nix/rust-toolchain.nix
        ];

        flake =
          let
            forAllSystems = nixpkgs.lib.genAttrs systems;

            # CodeTracer Python recorder package (available for both packages and devShells)
            codetracerPythonRecorderForSystem = system: codetracer-python-recorder.packages.${system}.codetracer-python-recorder;

            # AI coding agent packages (shared between packages and devShells)
            aiCodingAgentsForSystem =
              system:
              let
                pkgs = import nixpkgs {
                  inherit system;
                  overlays = [ rust-overlay.overlays.default ];
                  config.allowUnfree = true;
                };
              in
              [
                pkgs.goose-cli
                pkgs.claude-code
                pkgs.gemini-cli
                pkgs.opencode
                pkgs.qwen-code
                pkgs.cursor-cli
                pkgs.windsurf
                codex.packages.${system}.codex-rs
                nix-ai-tools.packages.${system}.copilot-cli
                nix-ai-tools.packages.${system}.crush
                nix-ai-tools.packages.${system}.groq-code-cli
                nix-ai-tools.packages.${system}.amp
              ];

          in
          {
            packages = forAllSystems (
              system:
              let
                pkgs = import nixpkgs {
                  inherit system;
                  overlays = [ rust-overlay.overlays.default ];
                  config.allowUnfree = true; # Allow unfree packages like claude-code
                };
                legacyAgentTask = pkgs.stdenv.mkDerivation {
                  pname = "agent-task-scripts";
                  version = "0.1.0";
                  src = null;
                  dontUnpack = true;
                  installPhase = ''
                    runHook preInstall

                    mkdir -p "$out/bin"
                    cp -r ${./bin}/. "$out/bin/"
                    chmod -R +x "$out/bin"

                    mkdir -p "$out/legacy"
                    cp -r ${./legacy}/. "$out/legacy/"

                    runHook postInstall
                  '';
                  meta = with pkgs.lib; {
                    description = "Legacy Ruby agent-task scripts bundled with their relative library files";
                    license = licenses.mit;
                  };
                };
                ah-script = pkgs.writeShellScriptBin "ah" ''
                  PATH=${
                    pkgs.lib.makeBinPath (
                      (aiCodingAgentsForSystem system)
                      ++ [
                        pkgs.asciinema
                      ]
                    )
                  }:$PATH
                  exec ${pkgs.ruby}/bin/ruby ${legacyAgentTask}/bin/agent-task "$@"
                '';
                get-task = pkgs.writeShellScriptBin "get-task" ''
                  exec ${pkgs.ruby}/bin/ruby ${legacyAgentTask}/bin/get-task "$@"
                '';
                start-work = pkgs.writeShellScriptBin "start-work" ''
                  exec ${pkgs.ruby}/bin/ruby ${legacyAgentTask}/bin/start-work "$@"
                '';
                legacy-cloud-agent-utils = pkgs.symlinkJoin {
                  name = "agent-utils";
                  paths = [
                    get-task
                    start-work
                  ];
                };
                fs = lib.fileset;
                rustWorkspaceFiles = fs.unions [
                  ./Cargo.toml
                  ./Cargo.lock
                  ./rust-toolchain.toml
                  ./clippy.toml
                  ./rustfmt.toml
                  ./src
                  ./crates
                  ./tests
                  ./assets
                  ./resources
                  ./specs
                ];
                rustWorkspaceSource = fs.toSource {
                  root = ./.;
                  fileset = rustWorkspaceFiles;
                };

                # Build the ah and ah-fs-snapshot-daemon binaries from the workspace
                ah-binary = pkgs.rustPlatform.buildRustPackage rec {
                  pname = "agent-harbor-cli";
                  version = "0.1.0";
                  src = rustWorkspaceSource;
                  cargoLock = {
                    lockFile = ./Cargo.lock;
                    outputHashes = {
                      "tui-textarea-0.7.0" = "sha256-2FQHtQ35Mgw8tMTUNq8rEBgPzIUYLhxx6wZGG0zjvdc=";
                      "vt100-0.16.2" = "sha256-BjcSXGw2Xc1QTB1uU9a2IsWdpoQBSjGt2dJLkm4t2ZE=";
                      "rmcp-0.9.0" = "sha256-F+vmm2DfMzDxAmDb/MbfwnVaepS7UH6XHVpxSrvOczY=";
                    };
                  };
                  nativeBuildInputs = [ pkgs.pkg-config ];
                  buildInputs = [
                    pkgs.openssl
                    pkgs.libseccomp
                    pkgs.zlib
                  ];
                  cargoBuildFlags = [
                    "--bin"
                    "ah"
                  ];
                  doCheck = false; # Skip tests for faster builds
                  meta = with pkgs.lib; {
                    description = "Agent Harbor CLI";
                    license = licenses.mit;
                    mainProgram = "ah";
                  };
                };
                ah-fs-snapshot-daemon-binary = pkgs.rustPlatform.buildRustPackage rec {
                  pname = "agent-harbor-daemon";
                  version = "0.1.0";
                  src = rustWorkspaceSource;
                  cargoLock = {
                    lockFile = ./Cargo.lock;
                    outputHashes = {
                      "tui-textarea-0.7.0" = "sha256-2FQHtQ35Mgw8tMTUNq8rEBgPzIUYLhxx6wZGG0zjvdc=";
                      "vt100-0.16.2" = "sha256-BjcSXGw2Xc1QTB1uU9a2IsWdpoQBSjGt2dJLkm4t2ZE=";
                      "rmcp-0.9.0" = "sha256-F+vmm2DfMzDxAmDb/MbfwnVaepS7UH6XHVpxSrvOczY=";
                    };
                  };
                  nativeBuildInputs = [ pkgs.pkg-config ];
                  buildInputs = [
                    pkgs.openssl
                    pkgs.libseccomp
                    pkgs.zlib
                  ];
                  cargoBuildFlags = [
                    "--bin"
                    "ah-fs-snapshots-daemon"
                  ];
                  doCheck = false; # Skip tests for faster builds
                  meta = with pkgs.lib; {
                    description = "Agent Harbor Filesystem Snapshot Daemon";
                    license = licenses.mit;
                  };
                };
                # Combine both binaries into the agent-harbor package
                agent-harbor = pkgs.symlinkJoin {
                  name = "agent-harbor";
                  paths = [
                    ah-binary
                    ah-fs-snapshot-daemon-binary
                  ];
                };
                pjdfstest = pkgs.stdenv.mkDerivation {
                  pname = "pjdfstest";
                  version = "master";
                  src = pjdfstest-src;
                  nativeBuildInputs = [ pkgs.autoreconfHook ];
                  buildInputs = [ pkgs.perl ];
                  installPhase = ''
                    mkdir -p $out/bin
                    cp pjdfstest $out/bin/
                    chmod +x $out/bin/pjdfstest
                  '';
                  meta = with pkgs.lib; {
                    description = "POSIX filesystem test suite";
                    license = licenses.bsd3;
                    platforms = platforms.all;
                  };
                };
              in
              {
                ah = ah-binary;
                ah-fs-snapshot-daemon = ah-fs-snapshot-daemon-binary;
                agent-harbor = agent-harbor;
                legacy-cloud-agent-utils = legacy-cloud-agent-utils;
                pjdfstest = pjdfstest;
                sosumi-docs-downloader = sosumi-docs-downloader.packages.${system}.sosumi-docs-downloader;
                codetracer-python-recorder = codetracerPythonRecorderForSystem system;
                default = ah-binary;
              }
            );

            devShells = forAllSystems (
              system:
              let
                pkgs = import nixpkgs {
                  inherit system;
                  overlays = [ rust-overlay.overlays.default ];
                  config.allowUnfree = true; # Allow unfree packages like claude-code
                };
                isLinux = pkgs.stdenv.isLinux;
                isDarwin = pkgs.stdenv.isDarwin;

                # CodeTracer Python recorder
                codetracerPythonRecorder = codetracer-python-recorder.packages.${system}.codetracer-python-recorder;

                # Yarn plugins
                yarnOutdated = pkgs.fetchurl {
                  # mskelton's redirect for Yarn v4 bundle
                  url = "https://go.mskelton.dev/yarn-outdated/v4";
                  sha256 = "1bhcl1sb8y7x29iy40v2gs23jkw6hyhqc3a3wbcq559jzmfqh49y";
                };

                # Common packages for all systems
                commonPackages = (aiCodingAgentsForSystem system) ++ [
                  config.allSystems.${system}.legacyPackages.rustToolchain
                  pkgs.just
                  pkgs.ruby
                  (pkgs.python3.withPackages (ps: [
                    ps.pyyaml
                    ps.pexpect
                    ps.ptyprocess
                    ps.pytest
                    ps.pyzmq
                    codetracerPythonRecorder
                  ]))
                  pkgs.ruby
                  pkgs.bundler
                  pkgs.rubocop
                  pkgs.git
                  pkgs.lazygit
                  pkgs.fossil
                  pkgs.mercurial
                  pkgs.jujutsu
                  pkgs.nodejs # for npx-based docson helper
                  pkgs.yarn-berry # Yarn PnP package manager
                  # Mermaid validation (diagram syntax)
                  (pkgs.nodePackages."@mermaid-js/mermaid-cli")
                  pkgs.noto-fonts

                  # Markdown linting & link/prose checking
                  pkgs.nodePackages.markdownlint-cli2
                  pkgs.lychee
                  pkgs.vale
                  pkgs.nodePackages.cspell
                  pkgs.nodePackages.prettier
                  pkgs.shfmt
                  pkgs.taplo
                  pkgs.addlicense

                  # Autotools for building C projects (pjdfstest, etc.)
                  pkgs.autoconf
                  pkgs.automake
                  pkgs.libtool

                  # pkgs.nodePackages."ajv-cli" # JSON Schema validator

                  # WebUI testing
                  # Playwright driver and browsers (bundled system libs for headless testing)
                  pkgs.playwright-driver # The driver itself
                  pkgs.playwright-driver.browsers # Bundled browsers with required libs
                  # Server management utilities for test orchestration
                  pkgs.netcat # For port checking (nc command)
                  pkgs.procps # For process management (pgrep, kill, etc.)
                  pkgs.process-compose # Process orchestration for API testing

                  # Terminal recording and sharing
                  pkgs.asciinema # Terminal session recorder
                  pkgs.fzf
                  pkgs.sqlite # For examining the databases of Cursor and VS Code

                  # ASCII art tools for logo conversion
                  pkgs.chafa

                  # Cargo tools
                  pkgs.cargo-outdated
                  pkgs.cargo-nextest
                  pkgs.cargo-insta

                  # Pre-commit tool for manual hook execution
                  pkgs.pre-commit

                  # OpenSSL for Rust crates that require it
                  pkgs.openssl

                  # MITM proxy for inspecting HTTPS traffic
                  pkgs.mitmproxy
                  # Network utilities for proxy setup
                  pkgs.netcat
                  # Additional compression libraries for mitmproxy addon
                  pkgs.python3Packages.brotli
                  pkgs.python3Packages.zstandard

                  # Filesystem testing
                  (self.packages.${system}.pjdfstest) # POSIX filesystem test suite for FUSE testing

                  # Native build tooling for Rust crates needing system libs
                  pkgs.pkg-config
                  pkgs.openssl.dev
                  pkgs.zlib.dev

                  # CodeTracer Python recorder
                  codetracerPythonRecorder
                ];

                ah-mux-test-tools =
                  [
                    pkgs.ncurses
                    pkgs.tmux
                    pkgs.screen
                    pkgs.zellij
                    pkgs.kitty
                  ]
                  ++ pkgs.lib.optionals isLinux [
                    pkgs.tilix
                  ];

                # GUI testing tools for headless environments
                gui-test-tools = [
                  pkgs.xorg.xorgserver # Xvfb for virtual display
                ];

                # Linux-specific packages
                linuxPackages = pkgs.lib.optionals isLinux [
                  # Use Chromium on Linux for mermaid-cli's Puppeteer
                  pkgs.chromium
                  # Linux-only filesystem utilities for snapshot functionality
                  pkgs.btrfs-progs # Btrfs utilities for subvolume snapshots
                  # Container runtimes for testing container workloads in sandbox
                  pkgs.docker
                  pkgs.podman
                  # System monitoring tools for performance tests
                  pkgs.procps # ps, top, etc. for memory monitoring
                  # Seccomp library for sandboxing functionality
                  pkgs.libseccomp # Required for seccomp-based sandboxing
                  pkgs.pkg-config # Required for libseccomp-sys to find libseccomp
                ];

                # macOS-specific packages
                darwinPackages = pkgs.lib.optionals isDarwin [
                  # Xcode environment wrapper
                  (pkgs.xcodeenv.composeXcodeWrapper {
                    versions = [ "16.0" ]; # Match your installed Xcode version
                  })
                  # Apple SDK frameworks
                  # pkgs.darwin.apple_sdk.frameworks.CoreFoundation
                  # pkgs.darwin.apple_sdk.frameworks.Security
                  # macOS-specific tools
                  pkgs.lima # Linux virtual machines on macOS
                  # Xcode project generation
                  pkgs.xcodegen
                  # Provide a reproducible Chrome for Puppeteer on macOS (unfree)
                  pkgs.google-chrome
                ];

                # All packages combined
                allPackages =
                  commonPackages
                  ++ linuxPackages
                  ++ darwinPackages
                  ++ config.allSystems.${system}.pre-commit.settings.enabledPackages
                  ++ ah-mux-test-tools
                  ++ gui-test-tools;

                # Platform-specific shell hook additions
                exportLinuxEnvVars =
                  if isLinux then
                    ''
                      export PLAYWRIGHT_LAUNCH_OPTIONS_EXECUTABLE_PATH="${pkgs.playwright-driver.browsers}/chromium-1181/chrome-linux/chrome"
                      export PLAYWRIGHT_CHROMIUM_EXECUTABLE="${pkgs.playwright-driver.browsers}/chromium-1181/chrome-linux/chrome"
                      export PUPPETEER_EXECUTABLE_PATH="${pkgs.chromium}/bin/chromium"
                    ''
                  else
                    "";

                exportDarwinEnvVars =
                  if isDarwin then
                    ''
                      # Clean up environment variables that might point to wrong tools
                      unset DEVELOPER_DIR
                      unset SDKROOT
                      export PLAYWRIGHT_LAUNCH_OPTIONS_EXECUTABLE_PATH="${pkgs.playwright-driver.browsers}/chromium-1181/chrome-mac/Chromium.app/Contents/MacOS/Chromium"
                      export PLAYWRIGHT_CHROMIUM_EXECUTABLE="${pkgs.playwright-driver.browsers}/chromium-1181/chrome-mac/Chromium.app/Contents/MacOS/Chromium"
                      export PUPPETEER_EXECUTABLE_PATH="${pkgs.google-chrome}/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
                    ''
                  else
                    "";

              in
              {
                default = pkgs.mkShell {
                  buildInputs = allPackages;

                  shellHook = ''
                    export PATH="$PATH:$PWD/target/debug"
                    # Install git pre-commit hook invoking our Nix-defined hooks
                    ${config.allSystems.${system}.pre-commit.settings.installationScript}

                    # Set default license for addlicense tool
                    export ADDLICENSE_LICENSE="AGPL-3.0-only"

                    # Load Yarn plugins
                    export YARN_PLUGINS="${yarnOutdated}"
                    echo "Loaded yarn-outdated plugin from Nix: $YARN_PLUGINS"

                    echo "Agent harbor development environment loaded${
                      if isDarwin then
                        " (macOS)"
                      else if isLinux then
                        " (Linux)"
                      else
                        ""
                    }"

                    # Playwright setup (use Nix-provided browsers, skip runtime downloads)
                    export PLAYWRIGHT_BROWSERS_PATH="${pkgs.playwright-driver.browsers}"
                    export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
                    export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
                    export PLAYWRIGHT_NODEJS_PATH="${pkgs.nodejs}/bin/node"

                    ${exportLinuxEnvVars}
                    ${exportDarwinEnvVars}

                    export PUPPETEER_PRODUCT=chrome
                    # Use the Nix-provided browser path (fully reproducible)

                    # Convenience function for Docson
                    docson () {
                      if command -v docson >/dev/null 2>&1; then
                        command docson "$@"
                        return
                      fi
                      if [ -n "''${IN_NIX_SHELL:-}" ]; then
                        echo "Docson is not available in this Nix dev shell. Add it to flake.nix (or choose an alternative) — no fallbacks allowed." >&2
                        return 127
                      fi
                      if command -v npx >/dev/null 2>&1; then
                        npx -y docson "$@"
                      else
                        echo "Docson not found and npx unavailable. Install Docson or enter nix develop with it provisioned." >&2
                        return 127
                      fi
                    }
                    echo "Tip: run: docson -d ./specs/schemas  # then open http://localhost:3000"
                    echo "🧪 Use: just mitm <program> [args...]  # to run programs behind mitmproxy with full HTTP(S) dumps"
                  '';
                };
              }
            );
          };
      }
    );
}
