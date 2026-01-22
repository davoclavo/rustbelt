# rustbelt

A set of Rust specific tools to provide enhanced tools via the MCP protocol. These tools provide IDE functionality like type hints, go-to-definition, and semantic analysis.

## Overview

rustbelt bridges rust-analyzer's powerful IDE capabilities with the Model Context Protocol, allowing AI assistants and other MCP clients to access Rust language intelligence. The server analyzes Rust projects and provides semantic information about code symbols, types, and structure.

## Usage

### MCP Server Mode (Recommended)

Start the server in stdio mode for MCP clients (default):

```bash
rustbelt serve
```

Or start in TCP mode for debugging:

```bash
rustbelt serve --tcp --host 127.0.0.1 --port 3001
```

### CLI Mode

Get type information directly from the command line:

```bash
rustbelt type-hint /path/to/file.rs 10 15
```

## Available Tools

| Tool Name          | Status | Description                                                                 | Parameters |
|--------------------|--------|-----------------------------------------------------------------------------|------------|
| `ruskel`           | Ready | Generate a Rust code skeleton for a crate, showing its public API structure | `target` (string), `features` (array), `all_features` (bool), `no_default_features` (bool), `private` (bool) |
| `get_type_hint`    | Alpha | Get type information for a symbol at cursor position                        | `file_path` (string), `line` (number 1-indexed), `column` (number 1-indexed) |
| `get_definition`   | Alpha | Get definition for symbol at cursor position                                | `file_path`, `line`, `column` |
| `get_completions`  | Alpha | Get code completion suggestions at cursor position                          | `file_path`, `line`, `column` |
| `rename_symbol`    | Alpha | Rename a symbol across the workspace                                        | `file_path`, `line`, `column`, `new_name` |
| `view_inlay_hints` | Alpha | View a file with embedded inlay hints, such as types or parameter names     | `file_path`, `start_line` (optional), `end_line` (optional) |
| `find_references`  | Alpha | Find all references to a symbol                                             | `file_path`, `line`, `column` |
| `get_assists`      | Alpha | Get available code assists (refactoring actions) at cursor position        | `file_path`, `line`, `column` |
| `apply_assist`     | Alpha | Apply a specific code assist by ID                                          | `file_path`, `line`, `column`, `assist_id` |

## Planned Improvements

### Tools

| Tool Name | Status | Description                         | Parameters                    |
|-----------|--------|-------------------------------------|-------------------------------|
| `get_signature_help` | Planned | Get function signature information  | `file_path`, `line`, `column` |
| `get_document_symbols` | Planned | Get all symbols in a document       | `file_path`                   |
| `get_workspace_symbols` | Planned | Search for symbols across workspace | `query`                       |
| `format_document` | Planned | Format a Rust document              | `file_path`                   |
| `get_diagnostics` | Planned | Get compiler errors and warnings    | `file_path`                   |
| `expand_macros` | Planned | Expand all macros in a file | `file_path`                    |

### General improvements

See TODO.md

## Requirements

- Rust 1.80+ nightly (uses Rust 2024 edition)
- A Rust project with `Cargo.toml` for analysis

## Community

Want to contribute? Have ideas or feature requests? Come tell us about it on
[Discord](https://discord.gg/fHmRmuBDxF).


## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Related Projects

- Powered by [tmcp](https://github.com/cortesi/tmcp)
- Relies on [ruskel](https://github.com/cortesi/ruskel) for generating Rust crate skeletons
- Built on top of [rust-analyzer](https://github.com/rust-lang/rust-analyzer) internal crates
