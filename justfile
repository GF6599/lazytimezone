# Run the TUI application
run *args:
  cargo run -- {{args}}

# Format and lint
fmt:
  cargo fmt
  cargo clippy

# Build release binaries and copy to dist/
dist:
  cargo build --release
  cross build --release --target x86_64-unknown-linux-gnu
  mkdir -p dist
  cp target/release/lazytimezone dist/
  cp target/x86_64-unknown-linux-gnu/release/lazytimezone dist/lazytimezone-linux-amd64

# Install the dist binary to the home directory
install: dist
  cp dist/lazytimezone ~/self-made-bin/
