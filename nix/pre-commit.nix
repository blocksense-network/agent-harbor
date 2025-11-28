{ inputs, ... }:
{
  imports = [
    inputs.git-hooks.flakeModule
  ];

  perSystem =
    { pkgs, self', ... }:
    {
      pre-commit = {
        settings = {
          hooks = {
            prettier = {
              enable = true;
              excludes = [ "\\.yarn" ];
              settings = {
                check = true;
                write = true;
                list-different = false;
                ignore-unknown = true;
                log-level = "warn";
              };
            };
            eslint = {
              enable = true;
              excludes = [
                ".obsidian"
                ".yarn"
                "electron-app"
              ];
              settings = {
                binPath = "yarn run -T eslint";
                extensions = "\\.[jt]s(x?)$";
              };
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
              packageOverrides = {
                rustfmt = self'.legacyPackages.rustToolchain;
                cargo = self'.legacyPackages.rustToolchain;
              };
              enable = true;
              name = "cargo fmt";
              entry = "cargo fmt --";
              language = "system";
              pass_filenames = true;
              files = "\\.rs$";
            };

            clippy = {
              enable = true;
              packageOverrides = {
                cargo = self'.legacyPackages.rustToolchain;
                clippy = self'.legacyPackages.rustToolchain;
              };
              excludes = [ "^vendor/" ];
              settings = {
                allFeatures = true;
                denyWarnings = true;
                extraArgs = "--workspace --exclude agent-client-protocol --tests --all-targets --no-deps";
                offline = false;
              };
            };

            # License header check and insertion
            spdx-addlicense = {
              enable = true;
              name = "SPDX headers (addlicense, fix then fail-on-change)";
              language = "system";
              pass_filenames = true;
              files = ''\.(c|cc|h|hpp|hh|cpp|go|rs|py|sh|bash|zsh|js|jsx|ts|tsx|yml|yaml|toml)$'';
              excludes = [ ".yarn" ];

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

            lychee-fast = {
              enable = true;
              name = "lychee-fast";
              entry = "lychee --no-progress --require-https --cache --config .lychee.toml";
              language = "system";
              pass_filenames = true;
              files = "\\.(md|mdx)$";
            };
          };

          tools = {
            # Commands invoked by hooks or scripts they call
            rubocop = pkgs.rubocop;
            markdownlint-cli2 = pkgs.nodePackages.markdownlint-cli2;
            just = pkgs.just; # for the lint-specs hook
            rg = pkgs.ripgrep; # used by check-merge-conflict
            mmdc = pkgs.nodePackages."@mermaid-js/mermaid-cli"; # used by md-mermaid-check via just lint-specs
          };
        };
      };
    };
}
