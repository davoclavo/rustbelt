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
use tmcp::{Result, ServerCtx, mcp_server, schema::CallToolResult, schemars, tool};
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
    async fn ensure_analyzer<P: AsRef<Path>>(&self, file_path: P) -> Result<()> {
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
    /// returns a single Rust source file that lists the
    /// *public API (or optionally private items) of any crate or module path, with all
    /// bodies stripped*. Useful for large‑language models that need to look up item
    /// names, signatures, derives, feature‑gated cfgs, and doc‑comments while writing
    /// or reviewing Rust code.
    ///
    /// ### When a model should call this tool
    /// 1. It needs a function/trait/struct signature it can't recall.
    /// 2. The user asks for examples or docs from a crate.
    /// 3. The model wants to verify what features gate a symbol.
    ///
    /// ### Target syntax examples
    /// - `serde`               →  latest serde on crates.io
    /// - `serde@1.0.160`      →  specific published version
    /// - `serde::de::Deserialize` →  narrow output to one module/type for small contexts
    /// - `/path/to/crate` or `/path/to/crate::submod` →  local workspace paths
    ///
    /// ### Output format
    /// Plain UTF‑8 text containing valid Rust code, with implementation omitted.
    ///
    /// ### Tips for LLMs
    /// - Request deep module paths (e.g. `tokio::sync::mpsc`) to keep the reply below
    ///   your token budget.
    /// - Pass `all_features=true` or `features=[…]` when a symbol is behind a feature gate.
    #[tool]
    async fn ruskel(&self, _ctx: &ServerCtx, params: RuskelParams) -> Result<CallToolResult> {
        let ruskel = Ruskel::new();

        match ruskel.render(
            &params.target,
            params.no_default_features,
            params.all_features,
            params.features.to_vec(),
            params.private,
        ) {
            Ok(skeleton) => Ok(CallToolResult::new().with_text_content(skeleton).is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error generating skeleton: {e}")).is_error(true)),
        }
    }

    /// Get type information for a symbol at a specific position in Rust code
    ///
    /// Provides detailed type information including variable types, function signatures,
    /// struct/enum definitions, and generic parameters. Use this when you need to understand
    /// the type of a symbol for code analysis, refactoring, or generating type-aware code.
    ///
    /// Returns human-readable type information or indicates if no type data is available.
    #[tool]
    async fn get_type_hint(
        &self,
        _ctx: &ServerCtx,
        params: CursorParams,
    ) -> Result<CallToolResult> {
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
            Ok(Some(type_info)) => Ok(CallToolResult::new().with_text_content(type_info.to_string()).is_error(false)),
            Ok(None) => Ok(CallToolResult::new().with_text_content("No type information available at this position").is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error getting type hint: {e}")).is_error(true)),
        }
    }

    /// Get definition location for a symbol at a specific position in Rust code
    ///
    /// Finds where symbols are defined - functions, types, variables, modules, macros,
    /// and more. Essential for code navigation and understanding symbol relationships.
    ///
    /// Returns definition locations as "file_path:line_number:column_number" format,
    /// or indicates if no definitions are found.
    #[tool]
    async fn get_definition(
        &self,
        _ctx: &ServerCtx,
        params: CursorParams,
    ) -> Result<CallToolResult> {
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

                Ok(CallToolResult::new().with_text_content(result_text).is_error(false))
            }
            Ok(None) => Ok(CallToolResult::new().with_text_content("No definitions found at this position").is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error getting definitions: {e}")).is_error(true)),
        }
    }

    /// Get completion suggestions at a specific position in Rust code
    ///
    /// Provides intelligent code completion suggestions including available methods,
    /// functions, variables, keywords, imports, and more based on the current context.
    ///
    /// Returns a list of completion suggestions with types and descriptions.
    #[tool]
    async fn get_completions(
        &self,
        _ctx: &ServerCtx,
        params: CursorParams,
    ) -> Result<CallToolResult> {
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

                Ok(CallToolResult::new().with_text_content(result_text).is_error(false))
            }
            Ok(None) => Ok(CallToolResult::new().with_text_content("No completions found at this position").is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error getting completions: {e}")).is_error(true)),
        }
    }

    /// Rename a symbol across the workspace
    ///
    /// Performs intelligent, workspace-wide symbol renaming that preserves code
    /// correctness and updates all references. Works with functions, types, variables,
    /// modules, macros, and more.
    ///
    /// Returns a summary of all changes made with file paths and line numbers, or
    /// explains why the rename is not possible.
    #[tool]
    async fn rename_symbol(
        &self,
        _ctx: &ServerCtx,
        params: RenameParams,
    ) -> Result<CallToolResult> {
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
            Ok(Some(rename_result)) => Ok(CallToolResult::new().with_text_content(rename_result.to_string()).is_error(false)),
            Ok(None) => Ok(CallToolResult::new().with_text_content("Symbol cannot be renamed at this position").is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error performing rename: {e}")).is_error(true)),
        }
    }

    /// View a Rust file with inlay hints embedded
    ///
    /// Enhances code readability by displaying inline type annotations and other
    /// helpful hints directly within the source code, including inferred types,
    /// parameter names, return types, and implicit conversions.
    ///
    /// If start_line and end_line are provided, only the specified range of lines
    /// will be returned with inlay hints. Both parameters are 1-based and inclusive.
    /// If neither parameter is provided, the entire file is processed.
    ///
    /// Returns the source file content (full file or specified range) with inlay hints embedded as inline annotations.
    #[tool]
    async fn view_inlay_hints(
        &self,
        _ctx: &ServerCtx,
        params: ViewInlayHintsParams,
    ) -> Result<CallToolResult> {
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
            Ok(annotated_content) => Ok(CallToolResult::new().with_text_content(annotated_content).is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error viewing inlay hints: {e}")).is_error(true)),
        }
    }

    /// Find all references to a symbol at a specific position in Rust code
    ///
    /// Searches for all references to a symbol (function, variable, type, etc.)
    /// throughout the workspace, including both the definition and all usage sites.
    /// Essential for understanding code dependencies and refactoring operations.
    ///
    /// Returns a list of reference locations with file paths, line numbers, and
    /// contextual information, or indicates if no references are found.
    #[tool]
    async fn find_references(
        &self,
        _ctx: &ServerCtx,
        params: CursorParams,
    ) -> Result<CallToolResult> {
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

                Ok(CallToolResult::new().with_text_content(result_text).is_error(false))
            }
            Ok(None) => Ok(CallToolResult::new().with_text_content("No references found at this position").is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error finding references: {e}")).is_error(true)),
        }
    }

    /// Get available code assists (code actions) at a specific position in Rust code
    ///
    /// Returns available assists like "extract function", "merge imports", "add missing impl", etc.
    /// These are context-sensitive refactoring and code transformation options that rust-analyzer
    /// can apply to improve or modify your code.
    ///
    /// Returns a list of available assists with their IDs, descriptions, and target ranges.
    #[tool]
    async fn get_assists(
        &self,
        _ctx: &ServerCtx,
        params: CursorParams,
    ) -> Result<CallToolResult> {
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

                Ok(CallToolResult::new().with_text_content(result_text).is_error(false))
            }
            Ok(None) => Ok(CallToolResult::new().with_text_content("No assists available at this position").is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error getting assists: {e}")).is_error(true)),
        }
    }

    /// Apply a specific code assist (code action) at a position in Rust code
    ///
    /// Takes an assist ID (from get_assists) and applies the corresponding code transformation
    /// to your source files. This will modify files on disk with the changes suggested by
    /// the assist.
    ///
    /// Common assists include "merge_imports", "extract_function", "add_missing_impl", etc.
    /// Returns a summary of the changes made to files.
    #[tool]
    async fn apply_assist(
        &self,
        _ctx: &ServerCtx,
        params: ApplyAssistParams,
    ) -> Result<CallToolResult> {
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
            Ok(Some(source_change)) => Ok(CallToolResult::new().with_text_content(source_change.to_string()).is_error(false)),
            Ok(None) => Ok(CallToolResult::new().with_text_content(format!(
                "Assist '{}' not available at this position",
                params.assist_id
            )).is_error(false)),
            Err(e) => Ok(CallToolResult::new().with_text_content(format!("Error applying assist: {e}")).is_error(true)),
        }
    }
}

pub async fn serve_stdio() -> Result<()> {
    tmcp::Server::default()
        .with_connection(Rustbelt::new)
        .serve_stdio()
        .await
}

pub async fn serve_tcp(addr: String) -> Result<()> {
    info!("Starting Rustbelt MCP server on {}", addr);

    tmcp::Server::default()
        .with_connection(Rustbelt::new)
        .serve_tcp(addr)
        .await
}
