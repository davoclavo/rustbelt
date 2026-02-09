//! rustbelt MCP Server
//!
//! This mcp provides rust-analyzer functionality via the Model Context
//! Protocol (MCP). It exposes IDE capabilities like type hints,
//! go-to-definition, and more as MCP tools.

use std::path::Path;
use std::sync::Arc;

use libruskel::Ruskel;
use librustbelt::{RustAnalyzerish, builder::RustAnalyzerishBuilder, entities::CursorCoordinates};
use serde::Deserialize;
use tmcp::{Result, ServerCtx, ToolResult, mcp_server, schema::CallToolResult, tool};
use tokio::sync::Mutex;
use tracing::info;

pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("VERGEN_GIT_SHA"),
    " (",
    env!("VERGEN_BUILD_DATE"),
    ")"
);

/// Parameters for the rename_symbol tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameParams {
    // TODO Do not nest CursorCoordinates here until tmcp properly reports schema
    /// Absolute path to the Rust source file
    pub file_path: String,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
    /// Optional symbol to find near the given coordinates.
    /// If provided, will search for this symbol within a tolerance box
    /// of +/- 5 lines/columns around the given coordinates.
    pub symbol: Option<String>,
    /// New name for the symbol
    pub new_name: String,
}

/// Parameters for the ruskel tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RuskelParams {
    /// Target specification (crate path, published crate name, or module path)
    pub target: String,
    /// Optional specific features to enable
    #[serde(default)]
    pub features: Vec<String>,
    /// Enable all features
    #[serde(default)]
    pub all_features: bool,
    /// Disable default features
    #[serde(default)]
    pub no_default_features: bool,
    /// Include private items in the skeleton
    #[serde(default)]
    pub private: bool,
}

/// Parameters for the view_inlay_hints tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ViewInlayHintsParams {
    /// Absolute path to the Rust source file
    pub file_path: String,
    /// Optional starting line number (1-based, inclusive)
    pub start_line: Option<u32>,
    /// Optional ending line number (1-based, inclusive)
    pub end_line: Option<u32>,
}

/// Parameters for the apply_assist tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApplyAssistParams {
    // TODO Do not nest CursorCoordinates here until tmcp properly reports schema
    /// Absolute path to the Rust source file
    pub file_path: String,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
    /// Optional symbol to find near the given coordinates.
    /// If provided, will search for this symbol within a tolerance box
    /// of +/- 5 lines/columns around the given coordinates.
    pub symbol: Option<String>,
    /// ID of the assist to apply
    pub assist_id: String,
}

/// Parameters for file-based tools (no cursor position needed)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileParams {
    /// Absolute path to the Rust source file
    pub file_path: String,
}

/// Parameters for symbol search
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchSymbolsParams {
    /// The search query (fuzzy matched against symbol names)
    pub query: String,
    /// Maximum number of results to return (default: 50)
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    50
}

/// Parameters for cursor-based tools
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CursorParams {
    /// Absolute path to the Rust source file
    pub file_path: String,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
    /// Optional symbol to find near the given coordinates.
    /// If provided, will search for this symbol within a tolerance box
    /// of +/- 5 lines/columns around the given coordinates.
    pub symbol: Option<String>,
}

/// Parameters for structural search and replace
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SsrParams {
    /// The SSR pattern. Format: `search_pattern ==>> replacement_pattern`
    /// Use `$name` for placeholders that match any AST node.
    ///
    /// Examples:
    /// - `foo($a) ==>> bar($a)` - Replace foo calls with bar calls
    /// - `$receiver.unwrap() ==>> $receiver?` - Replace unwrap with ?
    /// - `rgba(0x3B82F633) ==>> colors::BLUE_BG` - Replace specific values
    pub pattern: String,
    /// Optional file path for name resolution context.
    /// If not provided, uses the first file in the workspace.
    pub context_file: Option<String>,
    /// If true, only show matches without applying changes (default: false)
    #[serde(default)]
    pub dry_run: bool,
}

/// Parameters for SSR search (find matches without replacement)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SsrSearchParams {
    /// The search pattern. Use `$name` for placeholders.
    ///
    /// Examples:
    /// - `rgba($val)` - Find all rgba() calls
    /// - `$receiver.unwrap()` - Find all .unwrap() calls
    /// - `println!($args)` - Find all println! calls
    pub pattern: String,
    /// Optional file path for name resolution context.
    pub context_file: Option<String>,
}

/// Rust-Analyzer MCP server connection
#[derive(Debug, Clone)]
pub struct Rustbelt {
    analyzer: Arc<Mutex<Option<RustAnalyzerish>>>,
}

impl Rustbelt {
    fn new() -> Self {
        Self {
            analyzer: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the analyzer if it hasn't been created yet
    async fn ensure_analyzer<P: AsRef<Path>>(
        &self,
        file_path: P,
    ) -> std::result::Result<(), tmcp::ToolError> {
        let mut analyzer_guard = self.analyzer.lock().await;
        if analyzer_guard.is_none() {
            // Create analyzer with file watching enabled for the long-running MCP server
            let analyzer = RustAnalyzerishBuilder::from_file(file_path)
                .expect("Failed to find root workspace from given file")
                .with_file_watching(true)
                .build()
                .expect("Failed to create analyzer with current directory");

            *analyzer_guard = Some(analyzer);
        }
        Ok(())
    }
}

#[mcp_server]
impl Rustbelt {
    /// Generate a Rust code skeleton for a crate, showing its public API structure
    ///
    /// Returns a Rust source file listing the public API (or optionally private items)
    /// of any crate or module path, with all bodies stripped. Useful for looking up
    /// signatures, derives, feature-gated cfgs, and doc-comments.
    ///
    /// ## When to use
    ///
    /// - You need a function/trait/struct signature you can't recall.
    /// - You want to see the full API surface of a crate or module.
    /// - You need to verify what features gate a symbol.
    ///
    /// ## Target syntax
    ///
    /// - `serde` → latest on crates.io
    /// - `serde@1.0.160` → specific version
    /// - `serde::de::Deserialize` → narrow to one module/type (keeps output small)
    /// - `/path/to/crate` or `/path/to/crate::submod` → local workspace paths
    ///
    /// ## Tips
    ///
    /// - Use deep module paths (e.g. `tokio::sync::mpsc`) to keep output small.
    /// - Pass `all_features=true` or `features=[…]` for feature-gated symbols.
    #[tool]
    async fn ruskel(&self, _ctx: &ServerCtx, params: RuskelParams) -> ToolResult {
        let ruskel = Ruskel::new();

        match ruskel.render(
            &params.target,
            params.no_default_features,
            params.all_features,
            params.features.to_vec(),
            params.private,
        ) {
            Ok(skeleton) => Ok(CallToolResult::new().with_text_content(skeleton)),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error generating skeleton: {e}"))
                .mark_as_error()),
        }
    }

    /// Get type information for a symbol at a specific position in Rust code
    ///
    /// Returns the resolved type of a symbol (variable, expression, function, etc.)
    /// including generic parameters. Useful for complex inferred types that aren't
    /// obvious from reading the code.
    ///
    /// ## When to use
    ///
    /// - Complex inferred types: iterator chains, builder patterns, generic instantiations.
    /// - `impl Trait` or trait object returns where the concrete type matters.
    /// - Resolving what `T` becomes in a specific generic context.
    ///
    /// ## When NOT to use
    ///
    /// - Type is obvious: `let s = String::new()`, `let n: u32 = 5`.
    /// - You need the full API signature — use `ruskel` instead.
    /// - You need the definition location — use `get_definition` instead.
    #[tool]
    async fn get_type_hint(&self, _ctx: &ServerCtx, params: CursorParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .get_type_hint(&cursor)
            .await
        {
            Ok(Some(type_info)) => {
                Ok(CallToolResult::new().with_text_content(type_info.to_string()))
            }
            Ok(None) => Ok(CallToolResult::new()
                .with_text_content("No type information available at this position")),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error getting type hint: {e}"))
                .mark_as_error()),
        }
    }

    /// Get definition location for a symbol at a specific position in Rust code
    ///
    /// Finds where a symbol is defined — functions, types, variables, modules, macros.
    /// Returns locations as "file_path:line_number:column_number".
    ///
    /// ## When to use
    ///
    /// - Navigating to symbols from external crates or distant modules.
    /// - Resolving re-exports or macro definitions to their actual source.
    ///
    /// ## When NOT to use
    ///
    /// - Definition is in the same file — just read it.
    /// - You need the full API surface — use `ruskel` instead.
    /// - You need the *type*, not the location — use `get_type_hint`.
    /// - You need all *usages* — use `find_references`.
    #[tool]
    async fn get_definition(&self, _ctx: &ServerCtx, params: CursorParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .get_definition(&cursor)
            .await
        {
            Ok(Some(definitions)) => {
                let result_text = definitions
                    .iter()
                    .map(|def| def.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(CallToolResult::new().with_text_content(result_text))
            }
            Ok(None) => Ok(
                CallToolResult::new().with_text_content("No definitions found at this position")
            ),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error getting definitions: {e}"))
                .mark_as_error()),
        }
    }

    /// Get completion suggestions at a specific position in Rust code
    ///
    /// Returns context-aware completion suggestions: methods, functions, variables,
    /// enum variants, imports, and keywords available at the cursor position.
    ///
    /// ## When to use
    ///
    /// - Discovering available methods on an unfamiliar type (cursor after `.`).
    /// - Listing enum variants (cursor after `EnumName::`).
    /// - Finding importable symbols from a module.
    ///
    /// ## When NOT to use
    ///
    /// - You already know the symbol name — just write the code.
    /// - You need the full API with signatures — use `ruskel` instead.
    /// - You need the type of a specific symbol — use `get_type_hint`.
    #[tool]
    async fn get_completions(&self, _ctx: &ServerCtx, params: CursorParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .get_completions(&cursor)
            .await
        {
            Ok(Some(completions)) => {
                let result_text = completions
                    .iter()
                    .map(|comp| comp.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(CallToolResult::new().with_text_content(result_text))
            }
            Ok(None) => Ok(
                CallToolResult::new().with_text_content("No completions found at this position")
            ),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error getting completions: {e}"))
                .mark_as_error()),
        }
    }

    /// Rename a symbol across the workspace
    ///
    /// Performs workspace-wide symbol renaming that updates all references. Works with
    /// functions, types, variables, struct fields, enum variants, modules, and macros.
    /// Writes changes to disk immediately.
    ///
    /// ## When to use
    ///
    /// - Symbol is referenced across multiple files or crates in the workspace.
    /// - Renaming struct fields, enum variants, or trait methods that propagate to
    ///   impl blocks, pattern matches, and call sites.
    ///
    /// ## When NOT to use
    ///
    /// - Symbol is used in one place — just edit directly.
    /// - Renaming files/directories — use shell commands.
    /// - Only renames semantic references, not string literals or comments.
    #[tool]
    async fn rename_symbol(&self, _ctx: &ServerCtx, params: RenameParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .rename_symbol(&cursor, &params.new_name)
            .await
        {
            Ok(Some(rename_result)) => {
                Ok(CallToolResult::new().with_text_content(rename_result.to_string()))
            }
            Ok(None) => Ok(CallToolResult::new()
                .with_text_content("Symbol cannot be renamed at this position")),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error performing rename: {e}"))
                .mark_as_error()),
        }
    }

    /// View a Rust file with inlay hints embedded
    ///
    /// Returns source code with inline type annotations, parameter names, and other
    /// hints embedded directly in the text. Use `start_line`/`end_line` to limit
    /// the range (1-based, inclusive). Without them, the entire file is processed.
    ///
    /// ## When to use
    ///
    /// - Iterator chains and closures where element types aren't obvious.
    /// - Destructured tuples from external APIs (e.g. `(FileId, (TextEdit, Option<SnippetEdit>))`).
    /// - Nested generics: `Arc<LineIndex>`, `Option<Vec<ReferenceInfo>>`.
    /// - Parameter name hints on unfamiliar API calls.
    ///
    /// ## When NOT to use
    ///
    /// - Types are obvious from context: `Cli::parse()`, `format!(...)`, nearby enum patterns.
    /// - Simple `let` bindings where the RHS makes the type self-evident.
    #[tool]
    async fn view_inlay_hints(&self, _ctx: &ServerCtx, params: ViewInlayHintsParams) -> ToolResult {
        self.ensure_analyzer(&params.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .view_inlay_hints(&params.file_path, params.start_line, params.end_line)
            .await
        {
            Ok(annotated_content) => Ok(CallToolResult::new().with_text_content(annotated_content)),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error viewing inlay hints: {e}"))
                .mark_as_error()),
        }
    }

    /// Find all references to a symbol at a specific position in Rust code
    ///
    /// Returns all semantic references to a symbol across the workspace, including
    /// the definition site and every usage. Results include file paths, line numbers,
    /// and surrounding context.
    ///
    /// ## When to use
    ///
    /// - Before refactoring or deleting — understand the blast radius across the workspace.
    /// - Checking if a struct field, trait method, or function can be safely changed.
    ///
    /// ## When NOT to use
    ///
    /// - You just need the definition location — use `get_definition`.
    /// - Searching for a string pattern, not a semantic symbol — use grep instead.
    /// - Symbol is obviously local (loop variable, short function) — just read the code.
    #[tool]
    async fn find_references(&self, _ctx: &ServerCtx, params: CursorParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .find_references(&cursor)
            .await
        {
            Ok(Some(references)) => {
                let result_text = references
                    .iter()
                    .map(|ref_info| ref_info.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(CallToolResult::new().with_text_content(result_text))
            }
            Ok(None) => {
                Ok(CallToolResult::new().with_text_content("No references found at this position"))
            }
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error finding references: {e}"))
                .mark_as_error()),
        }
    }

    /// Get available code assists (code actions) at a specific position in Rust code
    ///
    /// Lists context-sensitive refactoring actions available at a cursor position.
    /// This is a read-only discovery step — use `apply_assist` with a returned ID
    /// to actually perform the transformation.
    ///
    /// Common assists: `extract_function`, `extract_variable`, `inline_call`,
    /// `add_missing_impl_members`, `merge_imports`.
    ///
    /// ## When to use
    ///
    /// - Before manually refactoring — check if an automated assist exists.
    /// - Exploring what transformations are possible at a given position.
    ///
    /// ## When NOT to use
    ///
    /// - You already know the assist ID — skip to `apply_assist`.
    /// - Simple text edits — just edit the file directly.
    #[tool]
    async fn get_assists(&self, _ctx: &ServerCtx, params: CursorParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .get_assists(&cursor)
            .await
        {
            Ok(Some(assists)) => {
                let result_text = assists
                    .iter()
                    .map(|assist| assist.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(CallToolResult::new().with_text_content(result_text))
            }
            Ok(None) => Ok(
                CallToolResult::new().with_text_content("No assists available at this position")
            ),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error getting assists: {e}"))
                .mark_as_error()),
        }
    }

    /// Apply a specific code assist (code action) at a position in Rust code
    ///
    /// Applies a code transformation identified by an assist ID from `get_assists`.
    /// Writes changes to disk immediately. Two-step workflow:
    /// 1. `get_assists` at a position → discover available assist IDs.
    /// 2. `apply_assist` with the chosen ID → apply the change.
    ///
    /// ## When to use
    ///
    /// - After `get_assists` returned an assist you want to apply.
    ///
    /// ## When NOT to use
    ///
    /// - Don't guess assist IDs — always call `get_assists` first.
    /// - Simple text edits — just edit the file directly.
    #[tool]
    async fn apply_assist(&self, _ctx: &ServerCtx, params: ApplyAssistParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .apply_assist(&cursor, &params.assist_id)
            .await
        {
            Ok(Some(source_change)) => {
                Ok(CallToolResult::new().with_text_content(source_change.to_string()))
            }
            Ok(None) => Ok(CallToolResult::new().with_text_content(format!(
                "Assist '{}' not available at this position",
                params.assist_id
            ))),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error applying assist: {e}"))
                .mark_as_error()),
        }
    }

    /// Check if code compiles and get diagnostics with suggested fixes
    ///
    /// Returns errors, warnings, and suggested quick-fixes for a file. Call this
    /// AFTER making edits to verify correctness. Each diagnostic includes inline
    /// fix suggestions so you can fix issues without additional tool calls.
    ///
    /// ## When to use
    ///
    /// - After editing Rust code to check for compile errors.
    /// - To discover warnings and quick-fix suggestions.
    /// - As part of an edit-check-fix loop.
    ///
    /// ## When NOT to use
    ///
    /// - For full `cargo build` diagnostics across the entire project — use `cargo check` via shell.
    /// - This only analyzes a single file at a time.
    #[tool]
    async fn get_diagnostics(&self, _ctx: &ServerCtx, params: FileParams) -> ToolResult {
        self.ensure_analyzer(&params.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .get_diagnostics(&params.file_path)
            .await
        {
            Ok(diagnostics) => {
                if diagnostics.is_empty() {
                    Ok(CallToolResult::new()
                        .with_text_content("No diagnostics — code looks clean."))
                } else {
                    let text = diagnostics
                        .iter()
                        .map(|d| d.to_string())
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    Ok(CallToolResult::new().with_text_content(text))
                }
            }
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error getting diagnostics: {e}"))
                .mark_as_error()),
        }
    }

    /// Understand a symbol completely — type, definition, implementations, callers, reference count
    ///
    /// Returns everything about a symbol in one call: its type, where it's defined,
    /// what implements it (or what traits it implements), who calls it, and how many
    /// references exist. Use this INSTEAD of calling get_type_hint + get_definition +
    /// find_references separately.
    ///
    /// ## When to use
    ///
    /// - Understanding an unfamiliar symbol before modifying it.
    /// - Assessing blast radius before a refactor.
    /// - Getting the full picture of a type/trait/function in one call.
    ///
    /// ## When NOT to use
    ///
    /// - You only need the type — use `get_type_hint` (lighter weight).
    /// - You need the full list of references — use `find_references`.
    #[tool]
    async fn analyze_symbol(&self, _ctx: &ServerCtx, params: CursorParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .analyze_symbol(&cursor)
            .await
        {
            Ok(analysis) => Ok(CallToolResult::new().with_text_content(analysis.to_string())),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error analyzing symbol: {e}"))
                .mark_as_error()),
        }
    }

    /// Get the structure of a file without reading it
    ///
    /// Returns all types, functions, impls, traits, and other items with their
    /// signatures and line numbers. Use BEFORE reading a file to decide which
    /// sections to focus on. Much cheaper than reading the whole file.
    ///
    /// ## When to use
    ///
    /// - Understanding a large file's structure before diving into specifics.
    /// - Finding which line range contains the function/type you need.
    /// - Getting a quick overview of a module's public API.
    ///
    /// ## When NOT to use
    ///
    /// - You need the full source code — use the Read tool.
    /// - You need the public API of an external crate — use `ruskel`.
    #[tool]
    async fn get_file_outline(&self, _ctx: &ServerCtx, params: FileParams) -> ToolResult {
        self.ensure_analyzer(&params.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .get_file_outline(&params.file_path)
            .await
        {
            Ok(items) => {
                if items.is_empty() {
                    Ok(
                        CallToolResult::new()
                            .with_text_content("No structure items found in file."),
                    )
                } else {
                    let text = items
                        .iter()
                        .map(|item| item.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(CallToolResult::new().with_text_content(text))
                }
            }
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error getting file outline: {e}"))
                .mark_as_error()),
        }
    }

    /// Find types, functions, or traits by name across the workspace
    ///
    /// Semantic fuzzy search — better than grep for finding Rust symbols. Returns
    /// symbol name, kind, file location, and container. Understands modules,
    /// re-exports, and symbol kinds.
    ///
    /// ## When to use
    ///
    /// - Finding a type/function/trait by name when you don't know which file it's in.
    /// - Exploring what symbols match a pattern (e.g., "Handler" finds AuthHandler, RequestHandler).
    /// - Navigating to a symbol by name instead of by file path.
    ///
    /// ## When NOT to use
    ///
    /// - Searching for string literals or comments — use grep.
    /// - You know the exact file — use `get_file_outline` or read the file.
    #[tool]
    async fn search_symbols(&self, _ctx: &ServerCtx, params: SearchSymbolsParams) -> ToolResult {
        // We need a file path to initialize the analyzer - use current dir
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        self.ensure_analyzer(&cwd).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .search_symbols(&params.query, params.limit)
            .await
        {
            Ok(results) => {
                if results.is_empty() {
                    Ok(CallToolResult::new()
                        .with_text_content(format!("No symbols found matching '{}'", params.query)))
                } else {
                    let text = results
                        .iter()
                        .map(|r| r.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(CallToolResult::new().with_text_content(text))
                }
            }
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error searching symbols: {e}"))
                .mark_as_error()),
        }
    }

    /// See what a macro expands to
    ///
    /// Shows the full expanded source code of a macro invocation. Use when you need
    /// to understand derive macros, proc macros, or macro_rules! invocations. Returns
    /// the macro name and its full expansion.
    ///
    /// ## When to use
    ///
    /// - Understanding what `#[derive(Debug)]` or custom derives generate.
    /// - Debugging macro_rules! expansions.
    /// - Inspecting proc macro output to understand generated code.
    ///
    /// ## When NOT to use
    ///
    /// - The macro is simple and well-known (e.g., `println!`, `vec!`).
    /// - You need to modify the macro itself — read the macro definition instead.
    #[tool]
    async fn expand_macro(&self, _ctx: &ServerCtx, params: CursorParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .expand_macro(&cursor)
            .await
        {
            Ok(Some(expansion)) => {
                Ok(CallToolResult::new().with_text_content(expansion.to_string()))
            }
            Ok(None) => Ok(CallToolResult::new()
                .with_text_content("No macro found at this position to expand.")),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error expanding macro: {e}"))
                .mark_as_error()),
        }
    }

    /// Get function parameter info at a call site
    ///
    /// Returns the function signature, parameter names and types, and which parameter
    /// the cursor is currently on. Use when writing function calls to verify argument
    /// order and types.
    ///
    /// ## When to use
    ///
    /// - Writing a function call and need to verify parameter order/types.
    /// - Checking what arguments a method expects at a specific call site.
    ///
    /// ## When NOT to use
    ///
    /// - You need the full function definition — use `get_definition` or `analyze_symbol`.
    /// - You need the full API of a type — use `ruskel`.
    #[tool]
    async fn get_signature_help(&self, _ctx: &ServerCtx, params: CursorParams) -> ToolResult {
        let cursor = CursorCoordinates {
            file_path: params.file_path,
            line: params.line,
            column: params.column,
            symbol: params.symbol,
        };
        self.ensure_analyzer(&cursor.file_path).await?;
        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .get_signature_help(&cursor)
            .await
        {
            Ok(Some(sig_info)) => Ok(CallToolResult::new().with_text_content(sig_info.to_string())),
            Ok(None) => Ok(CallToolResult::new()
                .with_text_content("No signature help available at this position.")),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("Error getting signature help: {e}"))
                .mark_as_error()),
        }
    }

    /// Structural Search and Replace (SSR) — semantic find-and-replace for Rust code
    ///
    /// Searches for code patterns using AST matching and optionally replaces them.
    /// Much more powerful than text-based find/replace because it understands Rust syntax.
    ///
    /// ## Pattern Syntax
    ///
    /// - Use `$name` for placeholders that match any expression/type/pattern
    /// - Format: `search_pattern ==>> replacement_pattern`
    /// - Placeholders in the replacement refer to what was matched
    ///
    /// ## Examples
    ///
    /// ```text
    /// # Replace function calls
    /// foo($a, $b) ==>> bar($b, $a)
    ///
    /// # Replace method chains
    /// $receiver.unwrap() ==>> $receiver?
    ///
    /// # Replace specific values with constants
    /// rgba(0x3B82F633) ==>> colors::BLUE_BG
    ///
    /// # Replace patterns
    /// if let Some($x) = $opt { $x } else { $default } ==>> $opt.unwrap_or($default)
    /// ```
    ///
    /// ## When to use
    ///
    /// - Bulk refactoring: renaming function calls, updating API usage patterns
    /// - Extracting repeated values into constants
    /// - Modernizing code patterns (e.g., `try!` to `?`, `unwrap` to `?`)
    /// - Any refactoring that involves replacing one code pattern with another
    ///
    /// ## When NOT to use
    ///
    /// - Simple text find/replace — use your editor
    /// - Renaming a single symbol — use `rename_symbol`
    /// - The pattern is purely textual with no structure
    #[tool]
    async fn ssr(&self, _ctx: &ServerCtx, params: SsrParams) -> ToolResult {
        // Get context file for analyzer initialization
        let init_path = params.context_file.as_deref().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string())
                .leak()
        });
        self.ensure_analyzer(init_path).await?;

        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .ssr(
                &params.pattern,
                params.context_file.as_deref(),
                params.dry_run,
            )
            .await
        {
            Ok(result) => Ok(CallToolResult::new().with_text_content(result.to_string())),
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("SSR error: {e}"))
                .mark_as_error()),
        }
    }

    /// Search for code patterns using SSR syntax (without replacement)
    ///
    /// Finds all occurrences of a structural pattern in the codebase.
    /// Use this to understand the scope of a refactoring before applying it.
    ///
    /// ## Pattern Syntax
    ///
    /// - Use `$name` for placeholders that match any expression/type/pattern
    /// - No `==>>` needed — this is search-only
    ///
    /// ## Examples
    ///
    /// ```text
    /// # Find all rgba() calls
    /// rgba($val)
    ///
    /// # Find all .unwrap() calls
    /// $receiver.unwrap()
    ///
    /// # Find all println! macro calls
    /// println!($args)
    ///
    /// # Find specific function calls
    /// std::fs::read_to_string($path)
    /// ```
    ///
    /// ## When to use
    ///
    /// - Before a bulk refactoring, to see what will be affected
    /// - Finding all usages of a particular code pattern
    /// - Auditing code for specific patterns (e.g., finding all `.unwrap()` calls)
    ///
    /// ## When NOT to use
    ///
    /// - Finding a single symbol's usages — use `find_references` instead
    /// - Searching for text strings or comments — use grep
    /// - You already know you want to replace — use `ssr` directly with `dry_run: true`
    #[tool]
    async fn ssr_search(&self, _ctx: &ServerCtx, params: SsrSearchParams) -> ToolResult {
        let init_path = params.context_file.as_deref().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string())
                .leak()
        });
        self.ensure_analyzer(init_path).await?;

        match self
            .analyzer
            .lock()
            .await
            .as_mut()
            .unwrap()
            .ssr_search(&params.pattern, params.context_file.as_deref())
            .await
        {
            Ok(matches) => {
                if matches.is_empty() {
                    Ok(CallToolResult::new().with_text_content(format!(
                        "No matches found for pattern: {}",
                        params.pattern
                    )))
                } else {
                    let text = format!(
                        "## Found {} matches\n\n{}",
                        matches.len(),
                        matches
                            .iter()
                            .map(|m| m.to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    Ok(CallToolResult::new().with_text_content(text))
                }
            }
            Err(e) => Ok(CallToolResult::new()
                .with_text_content(format!("SSR search error: {e}"))
                .mark_as_error()),
        }
    }
}

pub async fn serve_stdio() -> Result<()> {
    tmcp::Server::new(Rustbelt::new).serve_stdio().await
}

pub async fn serve_tcp(addr: String) -> Result<()> {
    info!("Starting Rustbelt MCP server on {}", addr);

    tmcp::Server::new(Rustbelt::new).serve_tcp(addr).await?;
    Ok(())
}
