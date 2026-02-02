//! librustbelt - Core library for rustbelt
//!
//! Provides simple interfaces for AI tools to interact with Rust code.

pub mod analyzer;
pub mod builder;
pub mod entities;
pub mod file_watcher;
pub mod utils;

pub use analyzer::RustAnalyzerish;
pub use builder::RustAnalyzerishBuilder;
pub use entities::{
    AssistInfo, AssistSourceChange, CallerInfo, CompletionItem, CursorCoordinates, DefinitionInfo,
    DiagnosticFix, DiagnosticInfo, FileChange, FileOutlineItem, MacroExpansion, ReferenceInfo,
    RenameResult, SignatureInfo, SymbolAnalysis, SymbolSearchResult, TextEdit, TypeHint,
};
pub use utils::RustAnalyzerUtils;

/// Result type alias for the library
pub type Result<T> = anyhow::Result<T>;
