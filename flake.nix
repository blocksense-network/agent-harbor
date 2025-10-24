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
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    git-hooks,
    nix-ai-tools,
    codex,
    sosumi-docs-downloader,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    forAllSystems = nixpkgs.lib.genAttrs systems;

    # AI coding agent packages (shared between packages and devShells)
    aiCodingAgentsForSystem = system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
        config.allowUnfree = true;
      };
    in [
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

  in {
    checks = forAllSystems (system: let
      pkgs = import nixpkgs { inherit system; };
      preCommit = git-hooks.lib.${system}.run {
        src = ./.;
        hooks = {
          # Markdown formatting (run first)
          prettier-md = {
            enable = true;
            name = "prettier --write (Markdown)";
            entry = "prettier --log-level warn --write";
            language = "system";
            pass_filenames = true;
            files = "\\.md$";
          };
          # Fast auto-fixers and sanity checks
          # Local replacements for common sanity checks (portable, no Python deps)
          check-merge-conflict = {
            enable = true;
            name = "check merge conflict markers";
            entry = ''
              bash -lc 'set -e; rc=0; for f in "$@"; do [ -f "$f" ] || continue; if rg -n "^(<<<<<<<|=======|>>>>>>>)" --color never --hidden --glob "!*.rej" --no-ignore-vcs -- "$f" >/dev/null; then echo "Merge conflict markers in $f"; rc=1; fi; done; exit $rc' --
            '';
            language = "system";
            pass_filenames = true;
            types = [ "text" ];
          };
          check-added-large-files = {
            enable = true;
            name = "check added large files (>1MB)";
            entry = ''
              bash -lc 'set -e; limit="$LIMIT"; [ -z "$limit" ] && limit=1048576; rc=0; for f in "$@"; do [ -f "$f" ] || continue; sz=$(stat -c %s "$f" 2>/dev/null || stat -f %z "$f"); if [ "$sz" -gt "$limit" ]; then echo "File too large: $f ($sz bytes)"; rc=1; fi; done; exit $rc' --
            '';
            language = "system";
            pass_filenames = true;
          };

          # Markdown: fix then lint
          markdownlint-fix = {
            enable = true;
            name = "markdownlint-cli2 (fix)";
            entry = "markdownlint-cli2 --fix";
            language = "system";
            pass_filenames = true;
            files = "\\.md$";
          };

          lint-specs = {
            enable = true;
            name = "Lint Markdown specs";
            # This hook calls markdownlint-cli2 directly with pass_filenames=true,
            # so it only processes modified .md files (incremental linting for fast pre-commits).
            # The --no-globs flag disables the .markdownlint-cli2.yaml config's "specs/**/*.md" glob,
            # ensuring only the specific files passed by pre-commit are linted.
            # The justfile's lint-specs target calls pre-commit with --all-files for comprehensive checks.
            entry = "markdownlint-cli2 --no-globs";
            language = "system";
            pass_filenames = true;
            files = "\\.md$";
          };

          # Spelling
          cspell = {
            enable = true;
            name = "cspell (cached)";
            entry = "cspell --no-progress --cache --config .cspell.json --exclude .obsidian/**";
            language = "system";
            pass_filenames = true;
            files = "\\.(md)$";
          };

          # Ruby formatting/linting (safe auto-correct)
          rubocop-autocorrect = {
            enable = false;
            name = "rubocop --safe-auto-correct";
            entry = "rubocop -A --force-exclusion";
            language = "system";
            pass_filenames = true;
            files = "\\.(rb|rake)$";
          };

          # Shell formatting
          shfmt = {
            enable = true;
            name = "shfmt";
            entry = "shfmt -w";
            language = "system";
            pass_filenames = true;
            files = "\\.(sh|bash)$";
          };

          # TOML formatting
          taplo-fmt = {
            enable = true;
            name = "taplo fmt";
            entry = "taplo fmt";
            language = "system";
            pass_filenames = true;
            files = "\\.toml$";
          };

          # Rust formatting
          rustfmt = {
            enable = true;
            name = "cargo fmt";
            entry = "cargo fmt --";
            language = "system";
            pass_filenames = true;
            files = "\\.rs$";
          };

          # License header check and insertion
          spdx-addlicense = {
            enable = true;
            name = "SPDX headers (addlicense, fix then fail-on-change)";
            language = "system";
            pass_filenames = true;
            files = ''\.(c|cc|h|hpp|hh|cpp|go|rs|py|sh|bash|zsh|js|jsx|ts|tsx|yml|yaml|toml)$'';

            # Use bash -lc so we can run a small script
            entry = ''
              bash -lc '
              set -euo pipefail
              # Run addlicense in-place on the files given by pre-commit
              "${pkgs.addlicense}/bin/addlicense" -l AGPL-3.0-only -s=only -c "Schelling Point Labs Inc" "$@"

              # If anything changed, fail the hook (so the commit stops).
              # Users re-stage and commit again, like with formatters.
              if ! git diff --exit-code -- "$@"; then
                echo
                echo "addlicense inserted SPDX headers in the files above."
                echo "Please review, stage the changes, and re-run your commit."
                exit 1
              fi
              ' --
            '';
          };

          # Fast link check on changed files (CI will run full scan)
          lychee-fast = {
            enable = true;
            name = "lychee (changed files)";
            entry = "lychee --no-progress --require-https --cache --config .lychee.toml";
            language = "system";
            pass_filenames = true;
            files = "\\.md$";
          };
        };
        # Ensure all hook entries (language = "system") have their executables available
        # when running in CI or via `nix flake check` (outside the dev shell).
        tools = {
          # Commands invoked by hooks or scripts they call
          prettier = pkgs.nodePackages.prettier;
          rubocop = pkgs.rubocop;
          shfmt = pkgs.shfmt;
          taplo = pkgs.taplo;
          lychee = pkgs.lychee;
          markdownlint-cli2 = pkgs.nodePackages.markdownlint-cli2;
          cspell = pkgs.nodePackages.cspell;
          just = pkgs.just; # for the lint-specs hook
          rg = pkgs.ripgrep; # used by check-merge-conflict
          mmdc = pkgs.nodePackages."@mermaid-js/mermaid-cli"; # used by md-mermaid-check via just lint-specs
          addlicense = pkgs.addlicense;
        };
      };
    in {
      pre-commit-check = preCommit;
    });
    packages = forAllSystems (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          config.allowUnfree = true; # Allow unfree packages like claude-code
        };
        ah-script = pkgs.writeShellScriptBin "ah" ''
          PATH=${pkgs.lib.makeBinPath ((aiCodingAgentsForSystem system) ++ [
            pkgs.asciinema
          ])}:$PATH
          exec ruby ${./bin/agent-task} "$@"
        '';
        get-task = pkgs.writeShellScriptBin "get-task" ''
          exec ${pkgs.ruby}/bin/ruby ${./bin/get-task} "$@"
        '';
        start-work = pkgs.writeShellScriptBin "start-work" ''
          exec ${pkgs.ruby}/bin/ruby ${./bin/start-work} "$@"
        '';
        legacy-cloud-agent-utils = pkgs.symlinkJoin {
          name = "agent-utils";
          paths = [get-task start-work];
        };
        # Build the ah and ah-fs-snapshot-daemon binaries from the workspace
        ah-binary = pkgs.rustPlatform.buildRustPackage rec {
          pname = "agent-harbor-cli";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = ["--bin" "ah"];
          doCheck = false; # Skip tests for faster builds
          meta = with pkgs.lib; {
            description = "Agent Harbor CLI";
            license = licenses.mit;
          };
        };
        ah-fs-snapshot-daemon-binary = pkgs.rustPlatform.buildRustPackage rec {
          pname = "agent-harbor-daemon";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = ["--bin" "ah-fs-snapshots-daemon"];
          doCheck = false; # Skip tests for faster builds
          meta = with pkgs.lib; {
            description = "Agent Harbor Filesystem Snapshot Daemon";
            license = licenses.mit;
          };
        };
        # Combine both binaries into the agent-harbor package
        agent-harbor = pkgs.symlinkJoin {
          name = "agent-harbor";
          paths = [ah-binary ah-fs-snapshot-daemon-binary];
        };
      in {
        ah = ah-binary;
        ah-fs-snapshot-daemon = ah-fs-snapshot-daemon-binary;
        agent-harbor = agent-harbor;
        legacy-cloud-agent-utils = legacy-cloud-agent-utils;
        sosumi-docs-downloader = sosumi-docs-downloader.packages.${system}.sosumi-docs-downloader;
        default = ah-script;
      }
    );

    devShells = forAllSystems (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
        config.allowUnfree = true; # Allow unfree packages like claude-code
      };
      isLinux = pkgs.stdenv.isLinux;
      isDarwin = pkgs.stdenv.isDarwin;

      # Yarn plugins
      yarnOutdated = pkgs.fetchurl {
        # mskelton's redirect for Yarn v4 bundle
        url = "https://go.mskelton.dev/yarn-outdated/v4";
        sha256 = "1bhcl1sb8y7x29iy40v2gs23jkw6hyhqc3a3wbcq559jzmfqh49y";
      };

      # Common packages for all systems
      commonPackages = (aiCodingAgentsForSystem system) ++ [
        # Rust toolchain
        (pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rustfmt" "clippy" "rust-src"];
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
        })

        pkgs.just
        pkgs.ruby
        (pkgs.python3.withPackages (ps: [
          ps.pyyaml
          ps.pexpect
          ps.ptyprocess
          ps.pytest
          ps.pyzmq
        ]))
        pkgs.ruby
        pkgs.bundler
        pkgs.rubocop
        pkgs.git
        pkgs.fossil
        pkgs.mercurial
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

        # pkgs.nodePackages."ajv-cli" # JSON Schema validator

        # WebUI testing
        # Playwright driver and browsers (bundled system libs for headless testing)
        pkgs.playwright-driver  # The driver itself
        pkgs.playwright-driver.browsers  # Bundled browsers with required libs
        # Server management utilities for test orchestration
        pkgs.netcat  # For port checking (nc command)
        pkgs.procps  # For process management (pgrep, kill, etc.)
        pkgs.process-compose  # Process orchestration for API testing

        # Terminal recording and sharing
        pkgs.asciinema # Terminal session recorder
        pkgs.fzf

        # ASCII art tools for logo conversion
        pkgs.chafa

        # Cargo tools
        pkgs.cargo-outdated
        pkgs.cargo-nextest
        pkgs.cargo-insta

        # Pre-commit tool for manual hook execution
        pkgs.pre-commit

        # Rust analyzer (matching the Nix-provided toolchain)
        pkgs.rust-analyzer

        # OpenSSL for Rust crates that require it
        pkgs.openssl

        # MITM proxy for inspecting HTTPS traffic
        pkgs.mitmproxy
        # Network utilities for proxy setup
        pkgs.netcat
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
          versions = [ "16.0" ];  # Match your installed Xcode version
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
      allPackages = commonPackages ++ linuxPackages ++ darwinPackages ++
                    self.checks.${system}.pre-commit-check.enabledPackages;

      # Platform-specific shell hook additions
      exportLinuxEnvVars = if isLinux then ''
        export PLAYWRIGHT_LAUNCH_OPTIONS_EXECUTABLE_PATH="${pkgs.playwright-driver.browsers}/chromium-1181/chrome-linux/chrome"
        export PLAYWRIGHT_CHROMIUM_EXECUTABLE="${pkgs.playwright-driver.browsers}/chromium-1181/chrome-linux/chrome"
        export PUPPETEER_EXECUTABLE_PATH="${pkgs.chromium}/bin/chromium"
      '' else "";

      exportDarwinEnvVars = if isDarwin then ''
        # Clean up environment variables that might point to wrong tools
        unset DEVELOPER_DIR
        unset SDKROOT
        export PLAYWRIGHT_LAUNCH_OPTIONS_EXECUTABLE_PATH="${pkgs.playwright-driver.browsers}/chromium-1181/chrome-mac/Chromium.app/Contents/MacOS/Chromium"
        export PLAYWRIGHT_CHROMIUM_EXECUTABLE="${pkgs.playwright-driver.browsers}/chromium-1181/chrome-mac/Chromium.app/Contents/MacOS/Chromium"
        export PUPPETEER_EXECUTABLE_PATH="${pkgs.google-chrome}/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
      '' else "";

    in {
      default = pkgs.mkShell {
        buildInputs = allPackages;

        shellHook = ''
          # Install git pre-commit hook invoking our Nix-defined hooks
          ${self.checks.${system}.pre-commit-check.shellHook}

          # Set default license for addlicense tool
          export ADDLICENSE_LICENSE="AGPL-3.0-only"

          # Load Yarn plugins
          export YARN_PLUGINS="${yarnOutdated}"
          echo "Loaded yarn-outdated plugin from Nix: $YARN_PLUGINS"

          echo "Agent harbor development environment loaded${if isDarwin then " (macOS)" else if isLinux then " (Linux)" else ""}"

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
    });
  };
}
