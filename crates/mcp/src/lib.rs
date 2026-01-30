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
            // Create a default analyzer for the current folder
            let analyzer = RustAnalyzerishBuilder::from_file(file_path)
                .expect("Failed to find root workspace from given file")
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
}

pub async fn serve_stdio() -> Result<()> {
    tmcp::Server::new(Rustbelt::new).serve_stdio().await
}

pub async fn serve_tcp(addr: String) -> Result<()> {
    info!("Starting Rustbelt MCP server on {}", addr);

    tmcp::Server::new(Rustbelt::new).serve_tcp(addr).await?;
    Ok(())
}
