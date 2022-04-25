#!/usr/bin/bash

set -e

# RUST_BACKTRACE=1 cargo run --release -- -v --test-folder test setup --num-files-to-generate 10000 --min-file-size 10 --max-file-size 50
# RUST_BACKTRACE=1 cargo run --release -- -v --test-folder test setup --num-files-to-generate 10 --min-file-size 10000 --max-file-size 100000
#RUST_BACKTRACE=1 cargo run --release -- -v --test-folder test test --num-iterations 100 --num-files-to-test 5000
#RUST_BACKTRACE=1 cargo run --release -- -v --test-folder test test --num-iterations 1 --num-files-to-test 100 --do-not-shuffle
RUST_BACKTRACE=1 cargo run --release -- -v --test-folder test test --num-iterations 1 --num-files-to-test 100
