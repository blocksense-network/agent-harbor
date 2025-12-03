#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Add words to both the project-scoped cspell configuration and vale Hunspell dictionary while keeping the allow-lists sorted.

The script inserts new words into both files, maintaining the union of words across both files
and preserving the ordering semantics (case-insensitive alphabetical order with stable case grouping).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Iterable, Sequence, Tuple

try:
    import yaml
except ImportError:
    try:
        import ruamel.yaml as yaml
    except ImportError:
        # Fallback YAML parsing for simple cases
        class SimpleYAML:
            @staticmethod
            def safe_load(stream):
                # Very basic YAML parser for our specific use case
                content = stream.read()
                lines = content.split('\n')
                data = {}
                current_list = None
                in_ignore = False

                for line in lines:
                    line = line.strip()
                    if not line or line.startswith('#'):
                        continue

                    if line.startswith('ignore:'):
                        in_ignore = True
                        current_list = []
                        data['ignore'] = current_list
                    elif in_ignore and line.startswith('- '):
                        current_list.append(line[2:])
                    elif ':' in line and not in_ignore:
                        key, value = line.split(':', 1)
                        data[key.strip()] = value.strip()

                return data

            @staticmethod
            def dump(data, stream, default_flow_style=False, sort_keys=False):
                for key, value in data.items():
                    if key == 'ignore' and isinstance(value, list):
                        stream.write(f"{key}:\n")
                        for item in value:
                            stream.write(f"  - {item}\n")
                    else:
                        stream.write(f"{key}: {value}\n")

        yaml = SimpleYAML()

DEFAULT_CSPELL_PATH = Path(__file__).resolve().parents[1] / ".cspell.json"
DEFAULT_VALE_DICT_DIR = Path(__file__).resolve().parents[1] / ".vale" / "config" / "dictionaries"

SortKey = Tuple[str, int, str]


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments for the script."""
    parser = argparse.ArgumentParser(
        description=(
            "Add one or more words to the cspell/vale allow-lists, or --sync to rewrite both"
            " lists in sorted order without adding new entries."
        ),
    )
    parser.add_argument(
        "words",
        nargs="*",
        help="Words to add to the allow-list (optional when using --sync).",
    )
    parser.add_argument(
        "--cspell",
        dest="cspell_path",
        type=Path,
        default=DEFAULT_CSPELL_PATH,
        help=f"Path to the cspell configuration file (default: {DEFAULT_CSPELL_PATH})",
    )
    parser.add_argument(
        "--vale-dict-dir",
        dest="vale_dict_dir",
        type=Path,
        default=DEFAULT_VALE_DICT_DIR,
        help=f"Path to the vale dictionaries directory (default: {DEFAULT_VALE_DICT_DIR})",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show the words that would be added without modifying the configuration.",
    )
    parser.add_argument(
        "--sync",
        action="store_true",
        help="Rewrite cspell and vale dictionaries from their current union without adding new words.",
    )
    return parser.parse_args(argv)


def normalize_words(words: Iterable[str]) -> list[str]:
    """Normalize user-supplied words by stripping whitespace and rejecting empties."""
    normalized: list[str] = []
    seen: set[str] = set()
    for raw in words:
        word = raw.strip()
        if not word:
            raise ValueError("Encountered an empty word argument after stripping whitespace.")
        if any(char.isspace() for char in word):
            raise ValueError(f"Word '{word}' contains internal whitespace; provide space-free tokens.")
        if word in seen:
            continue
        normalized.append(word)
        seen.add(word)
    return normalized


def sort_key(word: str) -> SortKey:
    """
    Compute the ordering key used in the cspell dictionary.

    The ordering is primarily case-insensitive. For words that only differ by casing, we
    group them to prefer lowercase variants first, then capitalized words, then camelCase
    or mixed-case variants, and finally all-uppercase forms.
    """
    lower = word.casefold()
    if word.islower():
        case_rank = 0
    elif word.isupper():
        case_rank = 3
    elif word[0].isupper() and word[1:].islower():
        case_rank = 1
    else:
        case_rank = 2
    return (lower, case_rank, word)


def add_words(existing_words: list[str], new_words: Sequence[str]) -> list[str]:
    """
    Merge `new_words` into `existing_words`, preserving the desired ordering semantics.

    The existing words array is re-sorted using the canonical ordering after new entries are
    appended. Returns the list of words that were actually added (duplicates are ignored).
    """
    added: list[str] = []
    existing_set = set(existing_words)
    for word in new_words:
        if word in existing_set:
            continue

        existing_words.append(word)
        existing_set.add(word)
        added.append(word)

    if added:
        existing_words.sort(key=sort_key)

    return added


def load_cspell_words(cspell_path: Path) -> tuple[dict, list[str]]:
    """Load the cspell configuration file and return its root object and words list."""
    if not cspell_path.exists():
        raise FileNotFoundError(f"cspell configuration not found at '{cspell_path}'.")

    with cspell_path.open("r", encoding="utf-8") as handle:
        try:
            cspell_data = json.load(handle)
        except json.JSONDecodeError as exc:
            raise ValueError(f"Failed to parse JSON from '{cspell_path}': {exc}") from exc

    words = cspell_data.get("words")
    if not isinstance(words, list) or not all(isinstance(item, str) for item in words):
        raise ValueError("The `.cspell.json` file does not contain a valid 'words' list.")

    return cspell_data, words


def write_cspell_words(cspell_path: Path, cspell_data: dict) -> None:
    """Persist the updated cspell configuration back to disk."""
    with cspell_path.open("w", encoding="utf-8") as handle:
        json.dump(cspell_data, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


def write_vale_hunspell_dict(vale_dict_dir: Path, words: list[str]) -> None:
    """Update the vale Hunspell dictionary with new words."""
    dict_file = vale_dict_dir / "en_custom.dic"

    # Read existing words if file exists
    existing_words = set()
    if dict_file.exists():
        with dict_file.open("r", encoding="utf-8") as f:
            lines = f.readlines()
            if lines:
                # Skip the first line (word count)
                for line in lines[1:]:
                    word = line.strip()
                    if word:
                        existing_words.add(word)

    # Add new words
    all_words = existing_words.union(set(words))
    sorted_words = sorted(all_words)

    # Write back the dictionary
    with dict_file.open("w", encoding="utf-8") as f:
        f.write(f"{len(sorted_words)}\n")
        for word in sorted_words:
            f.write(f"{word}\n")

    # Ensure affix file exists
    aff_file = vale_dict_dir / "en_custom.aff"
    if not aff_file.exists():
        with aff_file.open("w", encoding="utf-8") as f:
            f.write("SET UTF-8\n\n# Basic affix file for custom dictionary\n")


def load_vale_words(vale_dict_dir: Path) -> list[str]:
    """Load all words from the vale Hunspell dictionary (ignoring count header)."""
    dict_file = vale_dict_dir / "en_custom.dic"
    if not dict_file.exists():
        return []
    with dict_file.open("r", encoding="utf-8") as f:
        lines = f.readlines()
    if not lines:
        return []
    return [ln.strip() for ln in lines[1:] if ln.strip()]


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)

    if not args.sync and not args.words:
        print("No words supplied and --sync not set; nothing to do.", file=sys.stderr)
        return 1

    new_words: list[str] = []
    if args.words:
        try:
            new_words = normalize_words(args.words)
        except ValueError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 1

    cspell_data, cspell_words = load_cspell_words(args.cspell_path)
    vale_words = load_vale_words(args.vale_dict_dir)

    existing_union = set(cspell_words) | set(vale_words)
    target_words_set = existing_union | set(new_words)

    target_words = sorted(target_words_set, key=sort_key)

    added = list(sorted(target_words_set - set(cspell_words), key=sort_key)) if new_words else []

    if args.dry_run:
        action = "sync" if args.sync else "add"
        print(f"[dry-run] Would {action} {len(target_words_set)} words; newly added: {added}")
        return 0

    cspell_data["words"] = target_words
    write_cspell_words(args.cspell_path, cspell_data)
    write_vale_hunspell_dict(args.vale_dict_dir, target_words)

    if args.sync and not new_words:
        print(f"Synced spell dictionaries: {len(target_words)} entries.")
    else:
        print(
            f"Added {len(added)} word(s) to cspell and vale Hunspell dictionary: "
            f"{', '.join(added) if added else '(none)'}"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())


def main(argv: Sequence[str] | None = None) -> int:
    """Program entry point."""
    try:
        args = parse_args(argv)
        new_words = normalize_words(args.words)
        if not new_words:
            print("No new words provided after normalization; nothing to do.", file=sys.stderr)
            return 0

        # Load words from cspell
        cspell_data, cspell_words = load_cspell_words(args.cspell_path)

        # Load words from vale Hunspell dictionary
        dict_file = args.vale_dict_dir / "en_custom.dic"
        vale_words = []
        if dict_file.exists():
            with dict_file.open("r", encoding="utf-8") as f:
                lines = f.readlines()
                if lines:
                    # Skip the first line (word count)
                    for line in lines[1:]:
                        word = line.strip()
                        if word:
                            vale_words.append(word)

        # Get the union of all existing words
        all_existing_words = list(set(cspell_words + vale_words))

        # Add new words to the union
        added = add_words(all_existing_words, new_words)

        if not added:
            print("No changes: all words were already present in both files.")
            return 0

        if args.dry_run:
            print("Dry run; the following words would be added:")
            for word in added:
                print(f"  {word}")
            return 0

        # Update both files with the complete sorted word list
        cspell_data["words"] = all_existing_words
        write_cspell_words(args.cspell_path, cspell_data)
        write_vale_hunspell_dict(args.vale_dict_dir, all_existing_words)

        print(
            f"Added {len(added)} word{'s' if len(added) != 1 else ''} to cspell and vale Hunspell dictionary: "
            + ", ".join(added),
        )
        return 0
    except (FileNotFoundError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
