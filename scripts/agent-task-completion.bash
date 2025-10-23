# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# bash completion for agent-task
_agent_task_complete() {
  local cur prev repo
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD - 1]}"
  opts="--push-to-remote --prompt --prompt-file --devshell -s"
  if [[ $cur == --* ]]; then
    COMPREPLY=($(compgen -W "$opts" -- "$cur"))
    return 0
  fi
  if [[ $prev == --devshell || $prev == -s ]]; then
    repo=$(git rev-parse --show-toplevel 2>/dev/null)
    if [[ -n $repo && -f $repo/flake.nix ]]; then
      shells=$(grep -E "^[[:space:]]*[A-Za-z0-9._-]+[[:space:]]*=\s*pkgs\.mkShell" "$repo/flake.nix" |
        sed -E 's/^[[:space:]]*([A-Za-z0-9._-]+).*/\1/')
      COMPREPLY=($(compgen -W "$shells" -- "$cur"))
    fi
    return 0
  fi
}
complete -F _agent_task_complete agent-task
