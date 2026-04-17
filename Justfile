# List available recipes
default:
    @just --list

# Run the application
run *ARGS:
    cargo run -- {{ARGS}}

# Run with debug logging
run-debug *ARGS:
    RUST_LOG=nanite_clip=debug cargo run -- {{ARGS}}

# Build release binary
build:
    cargo build --release

# Run all checks (same as CI)
check: fmt-check clippy test

# Run tests
test:
    cargo test

# Run clippy lints
clippy:
    cargo clippy --all-targets -- -D warnings

# Check formatting
fmt-check:
    cargo fmt --check

# Format code
fmt:
    cargo fmt

# Install Linux desktop metadata into ~/.local/share for local testing
install-desktop:
    ./scripts/install-local-linux-desktop-integration.sh

# Clean build artifacts
clean:
    cargo clean

# Tag and release a new version (requires git-cliff)
release VERSION:
    ./scripts/release.sh {{VERSION}}
