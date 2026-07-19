set dotenv-load
set shell := ["bash", "-uc"]

# Show available recipes
default:
    @just --list

# Run the TUI application
[group('run')]
run *args:
  cargo run -- {{args}}

# Format and lint
[group('dev')]
fmt:
  cargo fmt
  cargo clippy

# Build release binaries and copy to dist/
[group('build')]
dist:
  cargo build --release
  cross build --release --target x86_64-unknown-linux-gnu
  mkdir -p dist
  cp target/release/lazytimezone dist/
  codesign --force --sign - dist/lazytimezone
  cp target/x86_64-unknown-linux-gnu/release/lazytimezone dist/lazytimezone-linux-amd64

# Install the dist binary to the home directory
[group('build')]
install: dist
  cp dist/lazytimezone ~/self-made-bin/
