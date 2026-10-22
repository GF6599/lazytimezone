set shell := ["bash", "-uc"]

# Show available recipes
default:
    @just --list

# Run the TUI application. Each argument reaches the app whole, spaces and all
[group('run')]
[positional-arguments]
run *args:
    cargo run -- "$@"

# The tape keeps its config under /tmp/lazytimezone-demo-config, which a fresh
# recording must not inherit.

# Record doc/demo.gif from demo.tape
[group('run')]
demo:
    rm -rf /tmp/lazytimezone-demo-config
    cargo build --release
    vhs demo.tape

# Lint with the same denial level CI uses
[group('dev')]
lint:
    cargo clippy --locked --all-targets -- -D warnings

# Format the workspace in place
[group('dev')]
fmt:
    cargo fmt --all

# Run the full test suite
[group('dev')]
test:
    cargo test --locked --all-targets

# CI installs the same version through `tools`, so the gate is one tool on
# every machine.
cargo_deny := "0.20.2"

# Install the tools the gate needs beyond the Rust toolchain
[group('dev')]
tools:
    cargo install --locked cargo-deny@{{ cargo_deny }}

# Check advisories, licences, duplicate crates and sources against deny.toml
[group('dev')]
deny:
    cargo deny --locked check

# `cargo fmt --check` belongs to no other recipe: `fmt` rewrites files,
# which is the opposite of what a gate should do.

# Run the whole quality gate: what CI and the commit hooks both call
[group('dev')]
check: lint test deny
    cargo fmt --all -- --check

# The IANA release that supplies zone.tab. Its cache file carries the release
# name, so a change here downloads the new table instead of reusing the old one.
tzdata := "tzdata2025b"

# Regenerate data/cities.tsv from the GeoNames dumps
[group('dev')]
gen-cities:
    mkdir -p target/geonames
    test -f target/geonames/cities15000.zip || curl -fsSL -o target/geonames/cities15000.zip https://download.geonames.org/export/dump/cities15000.zip
    unzip -o -q target/geonames/cities15000.zip -d target/geonames
    test -f target/geonames/cities1000.zip || curl -fsSL -o target/geonames/cities1000.zip https://download.geonames.org/export/dump/cities1000.zip
    unzip -o -q target/geonames/cities1000.zip -d target/geonames
    test -f target/geonames/admin1CodesASCII.txt || curl -fsSL -o target/geonames/admin1CodesASCII.txt https://download.geonames.org/export/dump/admin1CodesASCII.txt
    test -f target/geonames/countryInfo.txt || curl -fsSL -o target/geonames/countryInfo.txt https://download.geonames.org/export/dump/countryInfo.txt
    test -f target/geonames/{{ tzdata }}-zone.tab || curl -fsSL https://data.iana.org/time-zones/releases/{{ tzdata }}.tar.gz | tar -xzO zone.tab > target/geonames/{{ tzdata }}-zone.tab.part && mv target/geonames/{{ tzdata }}-zone.tab.part target/geonames/{{ tzdata }}-zone.tab
    cp target/geonames/{{ tzdata }}-zone.tab target/geonames/zone.tab
    cargo run --bin gen_cities -- target/geonames data/cities.tsv

# The Natural Earth release that supplies the land shapes. Its cache file
# carries the release name, as the zone.tab cache does.
natural_earth := "v5.1.2"

# Regenerate data/land.tsv from the Natural Earth land shapes
[group('dev')]
gen-land:
    mkdir -p target/natural-earth
    test -f target/natural-earth/{{ natural_earth }}-ne_110m_land.geojson || curl -fsSL -o target/natural-earth/{{ natural_earth }}-ne_110m_land.geojson https://raw.githubusercontent.com/nvkelso/natural-earth-vector/{{ natural_earth }}/geojson/ne_110m_land.geojson
    cargo run --bin gen_land -- target/natural-earth/{{ natural_earth }}-ne_110m_land.geojson data/land.tsv

# Build the release binary, sign it ad hoc, and copy it to dist/
[group('deploy')]
build:
    cargo build --locked --release
    mkdir -p dist
    cp target/release/lazytimezone dist/
    codesign --force --sign - dist/lazytimezone

# Install the dist binary to ~/self-made-bin/
[group('deploy')]
ship: build
    cp dist/lazytimezone ~/self-made-bin/
