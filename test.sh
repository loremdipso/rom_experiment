#!/usr/bin/bash

set -e

# RUST_BACKTRACE=1 cargo run --release -- -v --test-folder test setup --num-files-to-generate 10000 --min-file-size 10 --max-file-size 100
RUST_BACKTRACE=1 cargo run --release -- -v --test-folder test test --num-iterations 100 --num-files-to-test 5000
