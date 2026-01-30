# Default recipe: show available commands
default:
    @just --list

# Build all crates in release mode
build:
    cargo build --release

# Build all crates in debug mode
build-debug:
    cargo build

# Run all tests
test:
    cargo nextest run --all

# Run tests with output shown
test-verbose:
    cargo nextest run --all --no-capture

# Run clippy lints
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run cargo check
check:
    cargo check --all

# Install the CLI locally
install:
    cargo install --path crates/cli

# Install the CLI in release mode with locked dependencies
install-release:
    cargo install --path crates/cli --locked

# Clean build artifacts
clean:
    cargo clean

# Run the CLI
run *ARGS:
    cargo run --release -p rustbelt -- {{ARGS}}

# Generate documentation
doc:
    cargo doc --all --no-deps

# Open documentation in browser
doc-open:
    cargo doc --all --no-deps --open

# Update dependencies
update:
    cargo update

# Check for outdated dependencies
outdated:
    cargo outdated

# Run all CI checks (format, lint, test)
ci: fmt-check lint test
