# Vim/Neovim — Terminal Integration

Drive a split layout inside Vim or Neovim using built‑in terminal buffers and scripting.

## Overview

- Products: Vim (8.0+) and Neovim
- Platforms: Linux, macOS, Windows
- Interfaces: Ex commands (`:terminal`), Vimscript/Lua APIs; Neovim RPC (`--listen`) [1][2][3]

## Capabilities Summary

- Splits: `:vsplit`, `:split`, window navigation with `<C-w>` commands. [1]
- Terminal: Vim `:terminal` (since 8.0), Neovim `termopen()`/`:terminal`. [1][2]
- Start commands per pane: pass command to `:terminal` or Neovim `termopen()`. [1][2]
- Send keys/text: Neovim `chansend()` to terminal job channel; Vim `term_sendkeys()` to terminal buffer. [3][4]
- Focus/activate: Vim/Neovim window commands.

## Creating a New Tab With Split Layout (Neovim Lua)

```
local task_id = os.getenv("TASK_ID") or "ABC"
vim.cmd("tabnew")
vim.cmd("file ah-task-" .. task_id)
vim.cmd("vsplit")
-- Right pane: TUI follow
vim.cmd("terminal ah tui --follow " .. task_id)
-- Focus left and create bottom split for logs
vim.cmd("wincmd h")
vim.cmd("split")
vim.cmd("terminal ah session logs " .. task_id .. " -f")
-- Optional: open editor in top-left (current buffer is terminal; open a new buffer)
vim.cmd("wincmd k")
vim.cmd("enew")
```

## Launching Commands in Each Pane

- Vim: `:terminal {cmd}` launches a PTY running `{cmd}`. [1]
- Neovim: `:terminal` or `vim.fn.termopen(cmd)` for programmatic control. [2]

## Scripting Interactive Answers (Send Keys)

- Neovim: obtain the terminal job channel with `vim.b.terminal_job_id` and call `vim.fn.chansend(chan, {"y\r"})`. [3]
- Vim: use `term_sendkeys(bufnr, "y\<CR>")`. [4]

## Focusing an Existing Task’s Pane/Window

- Set the tab title or buffer name to `ah-task-<id>` and navigate by cycling tabs/buffers; external focus is handled by the terminal emulator/OS.

## Programmatic Control Interfaces

- Neovim: `nvim --headless --listen /tmp/nvim.sock` starts an RPC server; external processes can drive it via `nvim --server /tmp/nvim.sock --remote-send` (or msgpack‑rpc clients). [5]
- Vim: `--servername` and `--remote` require `+clientserver` builds on X11/Windows. [6]

## Detection and Version Compatibility

- Verify `:echo has('terminal')` in Vim (needs 8.0+). [1]
- Neovim supports these APIs in current stable 0.9+; `--listen` is documented in the Neovim manpage. [2][5]

## Cross‑Platform Notes

- Windows: both Vim and Neovim support terminal buffers; use PowerShell commands as needed.

## Example: TUI Follow Flow

Set `TASK_ID` in the environment, run the Lua above inside Neovim (or adapt to Vimscript) to create splits and start `ah tui --follow <id>`.

## References

1. Vim `:terminal` help: [vimhelp terminal][1]
2. Neovim `:terminal`/API: [Neovim terminal emulator][2]
3. Neovim `chansend()`: [Neovim eval chansend()][3]
4. Vim `term_sendkeys()`: [vimhelp term_sendkeys()][4]
5. Neovim `--listen` (RPC): [Neovim nvim(1) man page][5]
6. Vim clientserver: [vimhelp remote][6]

[1]: https://vimhelp.org/terminal.txt.html
[2]: https://neovim.io/doc/user/nvim_terminal_emulator.html
[3]: https://neovim.io/doc/user/eval.html#chansend()
[4]: https://vimhelp.org/terminal.txt.html#term_sendkeys%28%29
[5]: https://manpages.ubuntu.com/manpages/noble/en/man1/nvim.1.html
[6]: https://vimhelp.org/remote.txt.html
