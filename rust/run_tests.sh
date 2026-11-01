#!/bin/bash
cd /home/s3pp3ku/Projects/kyber-cursor/rust
export RUSTC=/home/s3pp3ku/.rustup/toolchains/1.89.0-x86_64-unknown-linux-gnu/bin/rustc
export CARGO=/home/s3pp3ku/.rustup/toolchains/1.89.0-x86_64-unknown-linux-gnu/bin/cargo
export RUSTSDOC=/home/s3pp3ku/.rustup/toolchains/1.89.0-x86_64-unknown-linux-gnu/bin/rustdoc

echo "=== TESTS (lib only):"
$CARGO test -p kyber-core --locked --lib 2>&1 | tail -6

echo
echo "=== CLIPPY (all targets):"
$CARGO clippy -p kyber-core --locked --all-targets -- -D warnings 2>&1 | tail -8
