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

# Regenerate data/cities.tsv from the GeoNames dumps
[group('dev')]
gen-cities:
    mkdir -p target/geonames
    test -f target/geonames/cities15000.zip || curl -fsSL -o target/geonames/cities15000.zip https://download.geonames.org/export/dump/cities15000.zip
    unzip -o -q target/geonames/cities15000.zip -d target/geonames
    test -f target/geonames/admin1CodesASCII.txt || curl -fsSL -o target/geonames/admin1CodesASCII.txt https://download.geonames.org/export/dump/admin1CodesASCII.txt
    test -f target/geonames/countryInfo.txt || curl -fsSL -o target/geonames/countryInfo.txt https://download.geonames.org/export/dump/countryInfo.txt
    cargo run --bin gen_cities -- target/geonames data/cities.tsv

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
