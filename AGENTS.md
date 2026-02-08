# CLAUDE.md

This file provides guidance for AI agents when working with code in this repository.

## Build & Test Commands

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Release build

# Test
cargo test                     # Run all tests
cargo test -p librustbelt      # Test just the core library
cargo test test_name           # Run specific test

# Lint
cargo clippy                   # Run clippy lints
cargo fmt                      # Format code

# Run
cargo run -- mcp               # Start MCP server (stdio mode, default)
cargo run -- mcp --tcp         # Start MCP server (TCP mode for debugging)
cargo run -- repl <path>       # Interactive REPL for a workspace
```

## Project Architecture

rustbelt exposes rust-analyzer IDE functionality via the Model Context Protocol (MCP). Three crates in `crates/`:

- **librustbelt** - Core library wrapping rust-analyzer. Key types:
  - `RustAnalyzerish` (`analyzer.rs`) - Main interface providing type hints, definitions, completions, references, renames, diagnostics, assists
  - `RustAnalyzerishBuilder` (`builder.rs`) - Builder pattern for initialization from file/workspace paths
  - `FileWatcher` (`file_watcher.rs`) - Handles VFS changes and file watching
  - `entities.rs` - Data types for API responses (TypeHint, DefinitionInfo, ReferenceInfo, etc.)

- **rustbelt-server** (mcp crate) - MCP server exposing librustbelt as tools via tmcp framework. Uses `#[mcp_server]` and `#[tool]` proc macros

- **cli** - Binary entry point with subcommands: `mcp`, `repl`, `analyzer`

## Key Patterns

**Cursor-based tools**: Most tools take `CursorParams` with `file_path`, `line`, `column` (1-based), and optional `symbol` for fuzzy matching within +/- 5 lines/columns

**Lazy analyzer initialization**: `Rustbelt::ensure_analyzer()` initializes on first tool use, discovering the workspace from the provided file path

**File watching**: Changes are applied via `file_watcher.drain_and_apply_changes()` before each operation

**SSR (Structural Search and Replace)**: Tools `ssr` and `ssr_search` provide semantic code transformation using rust-analyzer's SSR engine. Pattern syntax: `search_pattern ==>> replacement_pattern` with `$name` placeholders. Example: `$receiver.unwrap() ==>> $receiver?`

## Dependencies

- Rust nightly (`nightly-2026-01-01` via rust-toolchain.toml), Edition 2024
- rust-analyzer crates: `ra_ap_*` at version 0.0.316
- MCP framework: `tmcp`
- Skeleton generation: `libruskel` (external git dependency)
- Uses devbox for environment (provides rustup, openssl, curl, pkg-config)
