#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Add words to the project-scoped cspell configuration while keeping the allow-list sorted.

The script inserts new words into the `words` list in `.cspell.json` without disturbing the
existing ordering semantics (case-insensitive alphabetical order with stable case grouping).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Iterable, Sequence, Tuple

DEFAULT_CSPELL_PATH = Path(__file__).resolve().parents[1] / ".cspell.json"

SortKey = Tuple[str, int, str]


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments for the script."""
    parser = argparse.ArgumentParser(
        description="Add one or more words to the cspell dictionary allow-list while preserving ordering.",
    )
    parser.add_argument(
        "words",
        nargs="+",
        help="Words to add to the allow-list.",
    )
    parser.add_argument(
        "--cspell",
        dest="cspell_path",
        type=Path,
        default=DEFAULT_CSPELL_PATH,
        help=f"Path to the cspell configuration file (default: {DEFAULT_CSPELL_PATH})",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show the words that would be added without modifying the configuration.",
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


def main(argv: Sequence[str] | None = None) -> int:
    """Program entry point."""
    try:
        args = parse_args(argv)
        new_words = normalize_words(args.words)
        if not new_words:
            print("No new words provided after normalization; nothing to do.", file=sys.stderr)
            return 0

        cspell_data, words = load_cspell_words(args.cspell_path)
        added = add_words(words, new_words)

        if not added:
            print("No changes: all words were already present in `.cspell.json`.")
            return 0

        if args.dry_run:
            print("Dry run; the following words would be added:")
            for word in added:
                print(f"  {word}")
            return 0

        write_cspell_words(args.cspell_path, cspell_data)
        print(
            f"Added {len(added)} word{'s' if len(added) != 1 else ''} to {args.cspell_path}: "
            + ", ".join(added),
        )
        return 0
    except (FileNotFoundError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

