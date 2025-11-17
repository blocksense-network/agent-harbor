#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# create-test-files.sh - Creates test files for AgentFS overlay isolation testing
# This script generates filesystem side effects to verify that overlay operations
# are properly isolated from the underlying filesystem

set -e # Exit on any error

echo "🔧 Starting comprehensive filesystem operations test..."

# 1. Create basic files with echo/cat
echo "Creating basic test files..."
mkdir -p test_dir
echo "test content" >test_file.txt
cat >another_file.txt <<'EOF'
This is content created with cat
Multiple lines
With various content
EOF

if true; then
  echo "This is a test error"
  exit 1
fi

# 2. Edit files with sed
echo "Editing files with sed..."
sed -i 's/test content/modified test content/' test_file.txt
sed -i '1a\Added line after first line' another_file.txt

# 3. Create Python script and run it
echo "Creating and running Python script..."
cat >modify_files.py <<'EOF'
#!/usr/bin/env python3
import os
import sys

print("Python script: Starting file modifications...")

# Read existing files
with open('test_file.txt', 'r') as f:
    content = f.read()
    print(f"Read from test_file.txt: {repr(content.strip())}")

# Modify files
with open('test_file.txt', 'a') as f:
    f.write("\nAppended by Python")

# Create new files
with open('python_created.txt', 'w') as f:
    f.write("Created by Python script\n")
    f.write("With multiple lines\n")
    f.write("Python version: " + sys.version + "\n")

# Create binary-like content
with open('binary_test.dat', 'wb') as f:
    f.write(b'\x00\x01\x02\x03\xff\xfe\xfd')
    f.write(b'Binary data mixed with text')

# Create nested directory and file
os.makedirs('python_nested', exist_ok=True)
with open('python_nested/deep_file.txt', 'w') as f:
    f.write("File in nested Python directory\n")

print("Python script: File modifications complete")
EOF

chmod +x modify_files.py
python3 modify_files.py

# 4. Create Ruby script and run it
echo "Creating and running Ruby script..."
cat >modify_files.rb <<'EOF'
#!/usr/bin/env ruby

puts "Ruby script: Starting file modifications..."

# Read existing files
begin
  content = File.read('test_file.txt')
  puts "Read from test_file.txt: #{content.strip.inspect}"
rescue => e
  puts "Error reading test_file.txt: #{e.message}"
end

# Modify files
File.open('test_file.txt', 'a') do |f|
  f.write("\nAppended by Ruby")
end

# Create new files
File.open('ruby_created.txt', 'w') do |f|
  f.write("Created by Ruby script\n")
  f.write("With multiple lines\n")
  f.write("Ruby version: #{RUBY_VERSION}\n")
end

# Create binary-like content
File.open('ruby_binary_test.dat', 'wb') do |f|
  f.write("\x00\x01\x02\x03\xFF\xFE\xFD")
  f.write("Ruby binary data")
end

# Create nested directory and file
Dir.mkdir('ruby_nested') unless Dir.exist?('ruby_nested')
File.open('ruby_nested/deep_file.txt', 'w') do |f|
  f.write("File in nested Ruby directory\n")
end

puts "Ruby script: File modifications complete"
EOF

chmod +x modify_files.rb
ruby modify_files.rb

# 5. More sed operations and grep verification
echo "Performing advanced file operations..."
echo "original line" >sed_test.txt
sed -i 's/original/REPLACED/g' sed_test.txt

# Create files with various tools for comprehensive testing
echo "Testing various file creation methods..."
printf "printf content\nwith multiple lines\n" >printf_file.txt
touch empty_file.txt
echo "content" >overwrite_file.txt && echo "appended" >>overwrite_file.txt

# Create a complex directory structure
echo "Creating complex directory structure..."
mkdir -p complex/{dir1,dir2/subdir,dir3/subdir/deep}
echo "file in dir1" >complex/dir1/file1.txt
echo "file in subdir" >complex/dir2/subdir/file2.txt
echo "deep file" >complex/dir3/subdir/deep/deep_file.txt

# Test file permissions
echo "Testing file permissions..."
echo "permission test" >perm_file.txt
chmod 755 perm_file.txt

echo "🔧 Comprehensive filesystem operations test completed successfully"
sed -i '$a\This is the last line' sed_test.txt

# 5. Create files in subdirectories
echo "Creating nested directory structure..."
mkdir -p test_dir/subdir
echo "content in subdirectory" >test_dir/subfile.txt
echo "more content in subdir" >test_dir/subdir/nested.txt

# 6. Use grep to verify content
echo "Verifying file contents with grep..."
if ! grep -q "modified test content" test_file.txt; then
  echo "ERROR: Expected content not found in test_file.txt"
  exit 1
fi

if ! grep -q "Created by Python" python_created.txt; then
  echo "ERROR: Python-created file missing expected content"
  exit 1
fi

if ! grep -q "REPLACED" sed_test.txt; then
  echo "ERROR: sed replacement failed"
  exit 1
fi

# 7. Test file permissions and metadata
echo "Testing file permissions and metadata..."
chmod 644 test_file.txt
touch timestamp_test.txt
echo "timestamp test content" >timestamp_test.txt

# 8. Create some edge case files
echo "Creating edge case files..."
echo "" >empty_file.txt
echo -e "line with spaces   \t" >whitespace_file.txt
echo "file with special chars: !@#$%^&*()" >special_chars.txt

echo "✅ All filesystem operations completed successfully"
echo "daemon reuse test successful"
