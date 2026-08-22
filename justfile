set dotenv-load
set shell := ["bash", "-uc"]

# Show available recipes
default:
    @just --list

# Run the TUI application
[group('run')]
run *args:
    cargo run -- {{ args }}

# Lint with the same denial level CI uses
[group('dev')]
lint:
    cargo clippy --all-targets -- -D warnings

# Format the workspace in place
[group('dev')]
fmt:
    cargo fmt --all

# Run the full test suite
[group('dev')]
test:
    cargo test --all-targets

# `cargo fmt --check` belongs to no other recipe: `fmt` rewrites files,
# which is the opposite of what a gate should do.

# Run the whole quality gate: what CI and the commit hooks both call
[group('dev')]
check: lint test
    cargo fmt --all -- --check

# Build release binaries and copy to dist/
[group('deploy')]
build:
    cargo build --release
    cross build --release --target x86_64-unknown-linux-gnu
    mkdir -p dist
    cp target/release/lazytimezone dist/
    codesign --force --sign - dist/lazytimezone
    cp target/x86_64-unknown-linux-gnu/release/lazytimezone dist/lazytimezone-linux-amd64

# Install the dist binary to ~/self-made-bin/
[group('deploy')]
ship: build
    cp dist/lazytimezone ~/self-made-bin/
