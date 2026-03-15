# Run the TUI application
run *args:
  cargo run -- {{args}}

# Format and lint
fmt:
  cargo fmt
  cargo clippy

# Build release binary and copy to dist/
dist:
  cargo build --release
  mkdir -p dist
  cp target/release/lazytimezone dist/

# Install the dist binary to the home directory
install: dist
  cp dist/lazytimezone ~/self-made-bin/
