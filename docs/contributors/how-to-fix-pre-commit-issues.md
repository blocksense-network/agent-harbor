# How to Fix Pre-commit Issues

This guide explains how to address common pre-commit hook failures, specifically focusing on cspell (spell checking) errors.

## cspell (Spell Checking) Issues

### Problem

Pre-commit fails with output like:

```
cspell --no-progress --cache --config .cspell.json --exclude .obsidian/** "**/*.md"
crates/some-file.md:17:76 - Unknown word (tmuxinator)
...
CSpell: Files checked: 190, Issues found: 148 in 26 files.
```

### Solution

1. **Check for spelling errors in your staged changes**:

   ```bash
   pre-commit run cspell
   ```

   This runs cspell only on files that are staged for commit (the files you've added with `git add`).

2. **Review the errors** - distinguish between:
   - **Legitimate spelling mistakes**: Fix the actual text
   - **Technical terms/library names**: Add to `.cspell.json` dictionary
   - **Code references**: Add acronyms, function names, etc.

3. **Add missing words to `.cspell.json`**:
   - Prefer `just allow-words newword anotherword` to add terms while keeping the list sorted (pass multiple words in one run; add `--dry-run` first if you want to preview the changes)
   - Alternatively, open `.cspell.json` (located in the repository root) and edit the `"words"` array directly, maintaining alphabetical order
   - Include technical terms, system constants, library names, and project-specific jargon

4. **Verify the fix**:

   ```bash
   pre-commit run cspell
   ```

   Should show: `Issues found: 0 in 0 files`

5. **Commit the changes**:
   - Stage both the fixed files (if any text was corrected) and `.cspell.json`
   - Pre-commit should now pass

### Common Word Categories to Add

- **Technical terms**: `tmuxinator`, `envsubst`, `println`, `eprintln`
- **System constants**: `RTLD`, `EISDIR`, `ENOTDIR`, `sendmsg`, `recvmsg`
- **Code references**: `Lline`, `Ccolumn`, `behaviour`, `prioritisation`
- **Library/project names**: `Helicone`, `Multimodal`, `scanability`
- **External dependencies**: `termion`, `tuirs`, `textwrap`, `execv`

### Notes

- Words should be added in **alphabetical order** for maintainability
- Only add words that are genuinely correct for this codebase
- The `.cspell.json` file uses US English (`"language": "en-US"`)
- Vendor directories (like `vendor/codex/`) are external dependencies and their spelling issues are usually ignored
