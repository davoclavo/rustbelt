# rustbelt

A set of Rust specific tools to provide enhanced tools via the MCP protocol. These tools provide IDE functionality like type hints, go-to-definition, and semantic analysis.

## Overview

rustbelt bridges rust-analyzer's powerful IDE capabilities with the Model Context Protocol, allowing AI assistants and other MCP clients to access Rust language intelligence. The server analyzes Rust projects and provides semantic information about code symbols, types, and structure.

## Usage

### MCP Server Mode (Recommended)

Start the server in stdio mode for MCP clients (default):

```bash
rustbelt mcp
```

Or start in TCP mode for debugging:

```bash
rustbelt mcp --tcp --host 127.0.0.1 --port 3001
```

### CLI Mode

Get type information directly from the command line:

```bash
rustbelt type-hint /path/to/file.rs 10 15
```

## Available Tools

| Tool Name | Description | Parameters |
|-----------|-------------|------------|
| `ruskel` | Generate a Rust code skeleton for a crate, showing its public API structure. | `target`, `features?`, `all_features?`, `no_default_features?`, `private?` |
| `get_diagnostics` | Check if code compiles. Returns errors, warnings, and suggested fixes with inline source changes. | `file_path` |
| `analyze_symbol` | Understand a symbol completely — type, definition, implementations, callers, reference count — in one call. | `file_path`, `line`, `column`, `symbol?` |
| `get_file_outline` | Get the structure of a file without reading it. Shows all types, functions, impls with signatures and line numbers. | `file_path` |
| `search_symbols` | Find types, functions, or traits by name across the workspace. Semantic fuzzy search. | `query`, `limit?` |
| `expand_macro` | See what a macro expands to — derive macros, proc macros, macro_rules! invocations. | `file_path`, `line`, `column`, `symbol?` |
| `get_signature_help` | Get function parameter info at a call site — names, types, and active parameter. | `file_path`, `line`, `column`, `symbol?` |
| `get_type_hint` | Get type information for a symbol at cursor position. | `file_path`, `line`, `column`, `symbol?` |
| `get_definition` | Get definition location for a symbol at cursor position. | `file_path`, `line`, `column`, `symbol?` |
| `get_completions` | Get code completion suggestions at cursor position. | `file_path`, `line`, `column`, `symbol?` |
| `rename_symbol` | Rename a symbol across the workspace. Writes changes to disk. | `file_path`, `line`, `column`, `symbol?`, `new_name` |
| `view_inlay_hints` | View a file with embedded inlay hints (types, parameter names). | `file_path`, `start_line?`, `end_line?` |
| `find_references` | Find all references to a symbol across the workspace. | `file_path`, `line`, `column`, `symbol?` |
| `get_assists` | Get available code assists (refactoring actions) at cursor position. | `file_path`, `line`, `column`, `symbol?` |
| `apply_assist` | Apply a specific code assist by ID. Writes changes to disk. | `file_path`, `line`, `column`, `symbol?`, `assist_id` |

## Planned

| Tool Name | Description | Parameters |
|-----------|-------------|------------|
| `format_document` | Format a Rust document | `file_path` |

## Requirements

- Rust nightly (pinned to `nightly-2026-01-01`, uses Rust 2024 edition)
- A Rust project with `Cargo.toml` for analysis

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Related Projects

- Powered by [tmcp](https://github.com/cortesi/tmcp)
- Relies on [ruskel](https://github.com/cortesi/ruskel) for generating Rust crate skeletons
- Built on top of [rust-analyzer](https://github.com/rust-lang/rust-analyzer) internal crates
