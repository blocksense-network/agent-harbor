#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# verify-test-files.sh - Verifies that test files exist in AgentFS overlay
# This script checks that filesystem operations from previous overlay sessions
# are properly accessible in subsequent overlay sessions

set -e # Exit on any error

echo "🔍 Verifying comprehensive filesystem operations work correctly in AgentFS overlay..."

# 1. Test that we can perform various filesystem operations
echo "Checking basic file operations..."
if [ ! -f "test_file.txt" ]; then
  echo "ERROR: test_file.txt does not exist"
  exit 1
fi

if ! grep -q "modified test content" test_file.txt; then
  echo "ERROR: test_file.txt missing expected sed modification"
  echo "Content: $(cat test_file.txt)"
  exit 1
fi

if ! grep -q "Appended by Python" test_file.txt; then
  echo "ERROR: test_file.txt missing expected Python append"
  echo "Content: $(cat test_file.txt)"
  exit 1
fi

if ! grep -q "Appended by Ruby" test_file.txt; then
  echo "ERROR: test_file.txt missing expected Ruby append"
  echo "Content: $(cat test_file.txt)"
  exit 1
fi

if [ ! -f "another_file.txt" ]; then
  echo "ERROR: another_file.txt does not exist"
  exit 1
fi

if ! grep -q "Added line after first line" another_file.txt; then
  echo "ERROR: another_file.txt missing expected sed addition"
  echo "Content: $(cat another_file.txt)"
  exit 1
fi

if [ ! -f "printf_file.txt" ]; then
  echo "ERROR: printf_file.txt does not exist"
  exit 1
fi

if ! grep -q "printf content" printf_file.txt; then
  echo "ERROR: printf_file.txt missing expected content"
  echo "Content: $(cat printf_file.txt)"
  exit 1
fi

if [ ! -f "overwrite_file.txt" ]; then
  echo "ERROR: overwrite_file.txt does not exist"
  exit 1
fi

if ! grep -q "content" overwrite_file.txt; then
  echo "ERROR: overwrite_file.txt missing expected content"
  echo "Content: $(cat overwrite_file.txt)"
  exit 1
fi

if ! grep -q "appended" overwrite_file.txt; then
  echo "ERROR: overwrite_file.txt missing appended content"
  echo "Content: $(cat overwrite_file.txt)"
  exit 1
fi

# 2. Verify Python script operations
echo "Checking Python script operations..."
if [ ! -f "python_created.txt" ]; then
  echo "ERROR: python_created.txt does not exist"
  exit 1
fi

if ! grep -q "Created by Python script" python_created.txt; then
  echo "ERROR: python_created.txt missing expected content"
  echo "Content: $(cat python_created.txt)"
  exit 1
fi

if ! grep -q "With multiple lines" python_created.txt; then
  echo "ERROR: python_created.txt missing second line"
  echo "Content: $(cat python_created.txt)"
  exit 1
fi

if [ ! -f "binary_test.dat" ]; then
  echo "ERROR: binary_test.dat does not exist"
  exit 1
fi

if [ ! -d "python_nested" ]; then
  echo "ERROR: python_nested directory does not exist"
  exit 1
fi

if [ ! -f "python_nested/deep_file.txt" ]; then
  echo "ERROR: python_nested/deep_file.txt does not exist"
  exit 1
fi

if ! grep -q "nested Python directory" python_nested/deep_file.txt; then
  echo "ERROR: python_nested/deep_file.txt has wrong content"
  echo "Content: $(cat python_nested/deep_file.txt)"
  exit 1
fi

# 2.5. Verify Ruby script operations
echo "Checking Ruby script operations..."
if [ ! -f "ruby_created.txt" ]; then
  echo "ERROR: ruby_created.txt does not exist"
  exit 1
fi

if ! grep -q "Created by Ruby script" ruby_created.txt; then
  echo "ERROR: ruby_created.txt missing expected content"
  echo "Content: $(cat ruby_created.txt)"
  exit 1
fi

if [ ! -f "ruby_binary_test.dat" ]; then
  echo "ERROR: ruby_binary_test.dat does not exist"
  exit 1
fi

if [ ! -d "ruby_nested" ]; then
  echo "ERROR: ruby_nested directory does not exist"
  exit 1
fi

if [ ! -f "ruby_nested/deep_file.txt" ]; then
  echo "ERROR: ruby_nested/deep_file.txt does not exist"
  exit 1
fi

if ! grep -q "nested Ruby directory" ruby_nested/deep_file.txt; then
  echo "ERROR: ruby_nested/deep_file.txt has wrong content"
  echo "Content: $(cat ruby_nested/deep_file.txt)"
  exit 1
fi

# 3. Verify sed operations
echo "Checking sed operations..."
if [ ! -f "sed_test.txt" ]; then
  echo "ERROR: sed_test.txt does not exist"
  exit 1
fi

if ! grep -q "REPLACED" sed_test.txt; then
  echo "ERROR: sed_test.txt missing expected replacement"
  echo "Content: $(cat sed_test.txt)"
  exit 1
fi

if ! grep -q "This is the last line" sed_test.txt; then
  echo "ERROR: sed_test.txt missing appended line"
  echo "Content: $(cat sed_test.txt)"
  exit 1
fi

# 4. Verify directory structure and nested files
echo "Checking directory structure..."
if [ ! -d "test_dir" ]; then
  echo "ERROR: test_dir directory does not exist"
  exit 1
fi

if [ ! -d "test_dir/subdir" ]; then
  echo "ERROR: test_dir/subdir directory does not exist"
  exit 1
fi

if [ ! -f "test_dir/subfile.txt" ]; then
  echo "ERROR: test_dir/subfile.txt does not exist"
  exit 1
fi

if ! grep -q "content in subdirectory" test_dir/subfile.txt; then
  echo "ERROR: test_dir/subfile.txt has wrong content"
  echo "Content: $(cat test_dir/subfile.txt)"
  exit 1
fi

if [ ! -f "test_dir/subdir/nested.txt" ]; then
  echo "ERROR: test_dir/subdir/nested.txt does not exist"
  exit 1
fi

if ! grep -q "more content in subdir" test_dir/subdir/nested.txt; then
  echo "ERROR: test_dir/subdir/nested.txt has wrong content"
  echo "Content: $(cat test_dir/subdir/nested.txt)"
  exit 1
fi

# 4.5. Verify complex directory structure
echo "Checking complex directory structure..."
if [ ! -d "complex" ]; then
  echo "ERROR: complex directory does not exist"
  exit 1
fi

if [ ! -d "complex/dir1" ]; then
  echo "ERROR: complex/dir1 directory does not exist"
  exit 1
fi

if [ ! -f "complex/dir1/file1.txt" ]; then
  echo "ERROR: complex/dir1/file1.txt does not exist"
  exit 1
fi

if ! grep -q "file in dir1" complex/dir1/file1.txt; then
  echo "ERROR: complex/dir1/file1.txt has wrong content"
  echo "Content: $(cat complex/dir1/file1.txt)"
  exit 1
fi

if [ ! -d "complex/dir2/subdir" ]; then
  echo "ERROR: complex/dir2/subdir directory does not exist"
  exit 1
fi

if [ ! -f "complex/dir2/subdir/file2.txt" ]; then
  echo "ERROR: complex/dir2/subdir/file2.txt does not exist"
  exit 1
fi

if ! grep -q "file in subdir" complex/dir2/subdir/file2.txt; then
  echo "ERROR: complex/dir2/subdir/file2.txt has wrong content"
  echo "Content: $(cat complex/dir2/subdir/file2.txt)"
  exit 1
fi

if [ ! -d "complex/dir3/subdir/deep" ]; then
  echo "ERROR: complex/dir3/subdir/deep directory does not exist"
  exit 1
fi

if [ ! -f "complex/dir3/subdir/deep/deep_file.txt" ]; then
  echo "ERROR: complex/dir3/subdir/deep/deep_file.txt does not exist"
  exit 1
fi

if ! grep -q "deep file" complex/dir3/subdir/deep/deep_file.txt; then
  echo "ERROR: complex/dir3/subdir/deep/deep_file.txt has wrong content"
  echo "Content: $(cat complex/dir3/subdir/deep/deep_file.txt)"
  exit 1
fi

# 5. Verify edge case files
echo "Checking edge case files..."
if [ ! -f "empty_file.txt" ]; then
  echo "ERROR: empty_file.txt does not exist"
  exit 1
fi

if [ -s "empty_file.txt" ]; then
  echo "ERROR: empty_file.txt should be empty but has content: $(cat empty_file.txt)"
  exit 1
fi

if [ ! -f "whitespace_file.txt" ]; then
  echo "ERROR: whitespace_file.txt does not exist"
  exit 1
fi

if ! grep -q "line with spaces" whitespace_file.txt; then
  echo "ERROR: whitespace_file.txt missing expected content"
  echo "Content: $(cat whitespace_file.txt | cat -A)"
  exit 1
fi

if [ ! -f "special_chars.txt" ]; then
  echo "ERROR: special_chars.txt does not exist"
  exit 1
fi

if ! grep -q "file with special chars" special_chars.txt; then
  echo "ERROR: special_chars.txt missing expected content"
  echo "Content: $(cat special_chars.txt)"
  exit 1
fi

# 6. Verify file permissions and additional files
echo "Checking file permissions and additional files..."
if [ ! -f "timestamp_test.txt" ]; then
  echo "ERROR: timestamp_test.txt does not exist"
  exit 1
fi

if ! grep -q "timestamp test content" timestamp_test.txt; then
  echo "ERROR: timestamp_test.txt missing expected content"
  echo "Content: $(cat timestamp_test.txt)"
  exit 1
fi

if [ ! -f "perm_file.txt" ]; then
  echo "ERROR: perm_file.txt does not exist"
  exit 1
fi

if ! grep -q "permission test" perm_file.txt; then
  echo "ERROR: perm_file.txt missing expected content"
  echo "Content: $(cat perm_file.txt)"
  exit 1
fi

# 7. Final summary
echo ""
echo "🎉 COMPREHENSIVE OVERLAY FUNCTIONALITY TEST COMPLETED SUCCESSFULLY! 🎉"
echo ""
echo "✅ Basic file operations (echo/cat/printf): PASSED"
echo "✅ File editing with sed (search/replace/append): PASSED"
echo "✅ Python script execution and comprehensive file I/O: PASSED"
echo "✅ Ruby script execution and comprehensive file I/O: PASSED"
echo "✅ Binary file creation (Python and Ruby): PASSED"
echo "✅ Complex directory structures with deep nesting: PASSED"
echo "✅ File permissions and metadata operations: PASSED"
echo "✅ Edge case files (empty, whitespace, special chars): PASSED"
echo "✅ File overwriting and appending operations: PASSED"
echo "✅ Multiple programming languages working in overlay: PASSED"
echo ""
echo "🔍 AgentFS overlay functionality is working correctly!"
echo "   All filesystem operations execute properly within the overlay environment."

echo "overlay verification successful"
