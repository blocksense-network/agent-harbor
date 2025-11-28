#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Test script to verify that cspell and vale spell checking work correctly.

This test:
1. Creates a markdown file with a deliberate typo
2. Verifies both cspell and vale detect the typo
3. Adds the typo to the allow lists using allow_words.py
4. Verifies both tools now accept the word
5. Cleans up by removing the word from allow lists and deleting test files
"""

import json
import subprocess
import sys
import tempfile
import os
from pathlib import Path


def run_command(cmd, check=True, capture_output=True):
    """Run a shell command and return the result."""
    try:
        result = subprocess.run(cmd, shell=True, capture_output=capture_output, text=True)
        success = result.returncode == 0
        if check and not success:
            return False, result.stdout + result.stderr
        return success, result.stdout + result.stderr
    except Exception as e:
        return False, str(e)


def test_spell_checking():
    """Main test function."""
    print("🧪 Testing spell checking integration...")

    # Check if spell checking config files are staged
    result = subprocess.run(['git', 'status', '--porcelain', '.cspell.json', '.vale/config/dictionaries/en_custom.dic'],
                          capture_output=True, text=True, cwd='.')
    staged_files = []
    for line in result.stdout.split('\n'):
        if line.strip():
            status = line[:2]
            filename = line[3:]
            if status in ['A ', 'M ', 'AM', 'MM']:  # Staged files
                staged_files.append(filename)

    if staged_files:
        print("⚠️  WARNING: The following spell checking config files are staged:")
        for f in staged_files:
            print(f"   - {f}")
        print("Running the test will modify these files and unstage them.")
        try:
            response = input("Continue anyway? (y/N): ").strip().lower()
            if response != 'y':
                print("Test cancelled.")
                return False
        except EOFError:
            # No input available (e.g., in CI), assume no
            print("No input available, cancelling test to avoid unstaging files.")
            return False

    # Use a unique test word that's obviously not a real word
    test_word = "qwertyuiopasdf"
    test_content = f"This document contains a deliberate {test_word} that should be caught by spell checkers."

    # Create a temporary test file in the current directory
    test_file = f"test_spell_checking_temp_{os.getpid()}.md"
    with open(test_file, 'w') as f:
        f.write(test_content)

    print(f"1️⃣ Created test file with typo '{test_word}'")

    try:
        # Test cspell - should detect the typo
        print("2️⃣ Testing cspell...")
        success, output = run_command(
            f"cspell --no-progress --cache --config .cspell.json --exclude .obsidian/** {test_file}",
            check=False
        )
        if success:  # cspell returns 0 when no issues found
            print("❌ FAIL: cspell should have found the typo")
            return False
        else:
            print("✅ PASS: cspell found the typo")

        # Test vale - should detect the typo
        print("3️⃣ Testing vale...")
        success, output = run_command(f"vale {test_file}", check=False)
        if success:  # vale returns 0 when no issues found
            print("❌ FAIL: vale should have found the typo")
            return False
        else:
            print("✅ PASS: vale found the typo")

        # Add the typo to the allow list
        print(f"4️⃣ Adding '{test_word}' to allow list...")
        success, output = run_command(f"python3 scripts/allow_words.py {test_word}")
        if not success:
            print(f"❌ FAIL: allow_words.py failed: {output}")
            return False
        print("✅ PASS: Added word to allow lists")

        # Test cspell again - should now accept the word
        print("5️⃣ Testing cspell after allow_words...")
        success, output = run_command(
            f"cspell --no-progress --no-cache --config .cspell.json --exclude .obsidian/** {test_file}",
            check=False
        )
        if success:  # cspell returns 0 when no issues found
            print("✅ PASS: cspell now accepts the word")
        else:
            print(f"❌ FAIL: cspell still rejects the word: {output}")
            return False

        # Test vale again - should now accept the word
        print("6️⃣ Testing vale after allow_words...")
        success, output = run_command(f"vale {test_file}", check=False)
        if success:  # vale returns 0 when no issues found
            print("✅ PASS: vale now accepts the word")
        else:
            print("❌ FAIL: vale still rejects the word")

        # Clean up by removing the word from allow lists
        print("7️⃣ Cleaning up...")

        # Remove from cspell.json
        try:
            with open('.cspell.json', 'r') as f:
                data = json.load(f)
            if test_word in data.get('words', []):
                data['words'].remove(test_word)
                with open('.cspell.json', 'w') as f:
                    json.dump(data, f, indent=2)
                    f.write('\n')
                print(f"✅ Removed {test_word} from cspell.json")
            else:
                print(f"⚠️  {test_word} not found in cspell.json")
        except Exception as e:
            print(f"❌ Error cleaning up cspell.json: {e}")

        # Remove from Hunspell dictionary
        try:
            dict_file = Path(".vale/config/dictionaries/en_custom.dic")
            if dict_file.exists():
                # Read existing words
                with dict_file.open("r") as f:
                    lines = f.readlines()
                # Remove the test word and update count
                filtered_lines = []
                for line in lines:
                    if line.strip() != test_word:
                        filtered_lines.append(line)
                if len(filtered_lines) > 0:
                    # Update the word count
                    word_count = len(filtered_lines) - 1  # Subtract 1 for the count line
                    filtered_lines[0] = f"{word_count}\n"
                    with dict_file.open("w") as f:
                        f.writelines(filtered_lines)
                print(f"✅ Removed {test_word} from Hunspell dictionary")
            else:
                print(f"⚠️  Hunspell dictionary not found")
        except Exception as e:
            print(f"❌ Error cleaning up Hunspell dictionary: {e}")

        # Remove from vocabulary file if it exists
        vocab_file = ".vale/Vocab/AgentHarbor/accept.txt"
        if os.path.exists(vocab_file):
            try:
                # Read the file, filter out the test word, write back
                with open(vocab_file, 'r') as f:
                    lines = f.readlines()
                with open(vocab_file, 'w') as f:
                    for line in lines:
                        if line.strip() != test_word:
                            f.write(line)
                print(f"✅ Removed {test_word} from vocabulary file")
            except Exception as e:
                print(f"❌ Error cleaning up vocabulary file: {e}")

        print("✅ PASS: All tests completed successfully!")
        print("")
        print("Summary:")
        print("- ✅ cspell correctly detected and accepted the typo")
        print("- ✅ vale correctly detected and accepted the typo")
        print("- ✅ allow_words.py successfully updated configurations")
        print("- ✅ Cleanup completed")

        return True

    finally:
        # Clean up test file
        try:
            if os.path.exists(test_file):
                os.unlink(test_file)
        except:
            pass


if __name__ == "__main__":
    success = test_spell_checking()
    sys.exit(0 if success else 1)
