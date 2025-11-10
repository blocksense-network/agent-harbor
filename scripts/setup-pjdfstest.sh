#!/usr/bin/env bash
echo "Setting up pjdfstest test suite..."
mkdir -p resources
cd resources

if [ -d "pjdfstest" ]; then
    echo "pjdfstest directory already exists, updating..."
    cd pjdfstest
    git pull
else
    echo "Cloning pjdfstest repository..."
    git clone https://github.com/pjd/pjdfstest.git
    cd pjdfstest
fi

echo "Building pjdfstest test suite..."
autoreconf -ifs
./configure
make pjdfstest

echo "pjdfstest suite ready!"
echo "To run tests against a mounted filesystem (requires root):"
echo "  cd /path/to/mounted/filesystem"
echo "  prove -rv resources/pjdfstest/tests"
echo ""
echo "Available test files:"
ls tests/ | head -10
