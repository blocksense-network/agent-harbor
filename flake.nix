{
  description = " Agent Harbor";

  inputs = {
    # Pinned to specific commit for Playwright compatibility
    # Playwright 1.52.0 expects chromium-1169, which is available in this commit
    # but not in current nixpkgs-unstable. Can be updated when Playwright version
    # is upgraded or when nixpkgs-unstable has compatible chromium version.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
    git-hooks.url = "github:cachix/git-hooks.nix";
    codex = {
      url = "git+file:./third-party/codex";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-overlay.follows = "rust-overlay";
    };
    sosumi-docs-downloader = {
      url = "git+https://github.com/blocksense-network/sosumi-docs-downloader.git";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-overlay.follows = "rust-overlay";
    };
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    git-hooks,
    codex,
    sosumi-docs-downloader,
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    forAllSystems = nixpkgs.lib.genAttrs systems;
  in {
    checks = forAllSystems (system: let
      pkgs = import nixpkgs { inherit system; };
      preCommit = git-hooks.lib.${system}.run {
        src = ./.;
        hooks = {
          # Markdown formatting (run first) - DISABLED due to code block whitespace issues
          prettier-md = {
            enable = false;
            name = "prettier --write (Markdown)";
            entry = "prettier --loglevel warn --write";
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
            entry = "just lint-specs";
            language = "system";
            pass_filenames = false;
          };

          # Spelling
          cspell = {
            enable = true;
            name = "cspell (cached)";
            entry = "cspell --no-progress --cache --config .cspell.json --exclude .obsidian/**";
            language = "system";
            pass_filenames = true;
            files = "\\.(md|rb|rake|ya?ml|toml|json)$";
          };

          # Ruby formatting/linting (safe auto-correct)
          rubocop-autocorrect = {
            enable = true;
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
          PATH=${pkgs.lib.makeBinPath [
            pkgs.goose-cli
            pkgs.claude-code
            codex.packages.${system}.codex-rs
            pkgs.asciinema
            # not available in the currently pinned older nixpkgs:
            # pkgs.gemini-cli # Gemini CLI
            # pkgs.opencode # OpenCode AI coding assistant
          ]}:$PATH
          exec ruby ${./bin/agent-task} "$@"
        '';
        get-task = pkgs.writeShellScriptBin "get-task" ''
          exec ${pkgs.ruby}/bin/ruby ${./bin/get-task} "$@"
        '';
        start-work = pkgs.writeShellScriptBin "start-work" ''
          exec ${pkgs.ruby}/bin/ruby ${./bin/start-work} "$@"
        '';
        agent-utils = pkgs.symlinkJoin {
          name = "agent-utils";
          paths = [get-task start-work];
        };
      in {
        ah = ah-script;
        agent-utils = agent-utils;
        sosumi-docs-downloader = sosumi-docs-downloader.packages.${system}.sosumi-docs-downloader;
        default = ah-script;
      }
    );

    apps = forAllSystems (system: {
      ah = {
        type = "app";
        program = "${self.packages.${system}.ah}/bin/ah";
      };
      default = self.apps.${system}.ah;
    });

    devShells = forAllSystems (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
        config.allowUnfree = true; # Allow unfree packages like claude-code
      };
      isLinux = pkgs.stdenv.isLinux;
      isDarwin = pkgs.stdenv.isDarwin;

      # Common packages for all systems
      commonPackages = [
        # Rust toolchain
        (pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rustfmt" "clippy"];
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
        pkgs.python3
        pkgs.python3Packages.pexpect
        pkgs.python3Packages.pytest
        pkgs.ruby
        pkgs.bundler
        pkgs.rubocop
        pkgs.git
        pkgs.fossil
        pkgs.mercurial
        pkgs.nodejs # for npx-based docson helper
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


        # pkgs.nodePackages."ajv-cli" # JSON Schema validator

        # WebUI testing
        # Playwright driver and browsers (bundled system libs for headless testing)
        pkgs.playwright-driver  # The driver itself
        pkgs.playwright-driver.browsers  # Bundled browsers with required libs
        # Server management utilities for test orchestration
        pkgs.netcat  # For port checking (nc command)
        pkgs.procps  # For process management (pgrep, kill, etc.)
        pkgs.process-compose  # Process orchestration for API testing
        # Note: playwright and tsx are installed via npm in individual packages

        # AI Coding Assistants (available in current nixpkgs)
        pkgs.goose-cli # Goose AI coding assistant
        pkgs.claude-code # Claude Code - agentic coding tool
        # pkgs.gemini-cli # Gemini CLI - not available in older nixpkgs
        codex.packages.${system}.codex-rs # OpenAI Codex CLI (local submodule)
        # pkgs.opencode # OpenCode AI coding assistant - not available in older nixpkgs
        # Terminal recording and sharing
        pkgs.asciinema # Terminal session recorder
        pkgs.fzf

        # ASCII art tools for logo conversion
        pkgs.chafa

        # Cargo tools
        pkgs.cargo-outdated
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
        export PLAYWRIGHT_LAUNCH_OPTIONS_EXECUTABLE_PATH="${pkgs.playwright-driver.browsers}/chromium-1169/chrome-linux/chrome"
        export PLAYWRIGHT_CHROMIUM_EXECUTABLE="${pkgs.playwright-driver.browsers}/chromium-1169/chrome-linux/chrome"
        export PUPPETEER_EXECUTABLE_PATH="${pkgs.chromium}/bin/chromium"
      '' else "";

      exportDarwinEnvVars = if isDarwin then ''
        # Clean up environment variables that might point to wrong tools
        unset DEVELOPER_DIR
        unset SDKROOT
        export PLAYWRIGHT_LAUNCH_OPTIONS_EXECUTABLE_PATH="${pkgs.playwright-driver.browsers}/chromium-1169/chrome-mac/Chromium.app/Contents/MacOS/Chromium"
        export PLAYWRIGHT_CHROMIUM_EXECUTABLE="${pkgs.playwright-driver.browsers}/chromium-1169/chrome-mac/Chromium.app/Contents/MacOS/Chromium"
        export PUPPETEER_EXECUTABLE_PATH="${pkgs.google-chrome}/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
      '' else "";

    in {
      default = pkgs.mkShell {
        buildInputs = allPackages;

        shellHook = ''
          # Install git pre-commit hook invoking our Nix-defined hooks
          ${self.checks.${system}.pre-commit-check.shellHook}
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
        '';
      };
    });
  };
}