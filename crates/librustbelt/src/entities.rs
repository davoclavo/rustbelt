use ra_ap_ide::LineCol;
use ra_ap_ide_db::SymbolKind;
use serde::{Deserialize, Serialize};
use std::fmt;

const TOLERANCE: u32 = 5;
/// Cursor coordinates for specifying position in a file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct CursorCoordinates {
    /// Absolute path to the Rust source file
    pub file_path: String,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
    /// Optional symbol to find near the given coordinates.
    /// If provided, will search for this symbol within a tolerance box
    /// of +/- 5 lines/columns around the given coordinates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

impl CursorCoordinates {
    /// Find the exact coordinates of a symbol within a tolerance box
    ///
    /// If a symbol is specified, searches for it within +/- 5 lines/columns
    /// of the given coordinates. Returns the refined coordinates or original
    /// coordinates if symbol is not found or not specified.
    pub fn resolve_coordinates(&self, file_content: &str) -> CursorCoordinates {
        if let Some(ref symbol) = self.symbol {
            self.find_symbol_in_tolerance_box(file_content, symbol)
                .unwrap_or_else(|| self.clone())
        } else {
            self.clone()
        }
    }

    /// Find a symbol within a tolerance box around the given coordinates
    fn find_symbol_in_tolerance_box(
        &self,
        file_content: &str,
        symbol: &str,
    ) -> Option<CursorCoordinates> {
        let lines: Vec<&str> = file_content.lines().collect();

        // Generate a range of line numbers to search, starting from the center line and expanding outwards
        let mut line_range = Vec::new();
        for offset in 0..=TOLERANCE {
            let actual_line_number = (self.line + offset) as usize;
            if actual_line_number <= lines.len() {
                line_range.push(actual_line_number);
            }
            if offset != 0 && self.line > offset {
                let actual_line_number = (self.line - offset) as usize;
                if actual_line_number > 0 {
                    line_range.push(actual_line_number);
                }
            }
        }

        // Search line by line within the tolerance box
        for actual_line_number in line_range {
            if let Some(line) = lines.get(actual_line_number - 1)
                && let Some(column_pos) = self.find_symbol_in_line(line, symbol, actual_line_number)
            {
                return Some(CursorCoordinates {
                    file_path: self.file_path.clone(),
                    line: actual_line_number as u32,
                    column: column_pos,
                    symbol: self.symbol.clone(),
                });
            }
        }

        None
    }

    /// Find a symbol within a line, considering column tolerance
    fn find_symbol_in_line(&self, line: &str, symbol: &str, line_number: usize) -> Option<u32> {
        // Find all occurrences of the symbol in the line
        let mut matches = Vec::new();
        let mut start = 0;
        while let Some(pos) = line[start..].find(symbol) {
            let absolute_pos = start + pos;
            matches.push(absolute_pos);
            start = absolute_pos + 1;
        }

        if matches.is_empty() {
            return None;
        }

        // If this is the center line, find the closest match to the target column
        if line_number == self.line as usize {
            let target_col = self.column as usize;
            let mut closest_pos = matches[0];
            let mut closest_distance = (closest_pos + 1).abs_diff(target_col);

            for &pos in &matches {
                let distance = (pos + 1).abs_diff(target_col);
                if distance < closest_distance {
                    closest_distance = distance;
                    closest_pos = pos;
                }
            }

            // Check if the closest match is within tolerance
            if closest_distance <= TOLERANCE as usize {
                return Some(closest_pos as u32 + 1);
            }
        }

        // If not the center line or no match within tolerance, return the first occurrence
        Some(matches[0] as u32 + 1)
    }
}

impl From<&CursorCoordinates> for LineCol {
    fn from(cursor: &CursorCoordinates) -> Self {
        LineCol {
            line: cursor.line.saturating_sub(1),
            col: cursor.column.saturating_sub(1),
        }
    }
}

/// Information about a definition location
#[derive(Debug, Clone)]
pub struct DefinitionInfo {
    /// Path to the file containing the definition
    pub file_path: String,
    /// Line number (1-based) where the definition starts
    pub line: u32,
    /// Column number (1-based) where the definition starts
    pub column: u32,
    /// Line number (1-based) where the definition ends
    pub end_line: u32,
    /// Column number (1-based) where the definition ends
    pub end_column: u32,
    /// Name of the defined symbol
    pub name: String,
    /// Kind of the symbol (function, struct, etc.)
    pub kind: Option<SymbolKind>,
    /// Content of the definition
    pub content: String,
    /// Canonical module path
    pub module: String,
    /// Rustdoc description, if available
    pub description: Option<String>,
}

/// Information about a rename operation result
#[derive(Debug, Clone)]
pub struct RenameResult {
    /// Files that will be changed by the rename operation
    pub file_changes: Vec<FileChange>,
}

/// Information about changes to a single file during rename
#[derive(Debug, Clone)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct FileChange {
    /// Path to the file that will be changed
    pub file_path: String,
    /// List of text edits to apply to this file
    pub edits: Vec<TextEdit>,
}

/// A single text edit within a file
#[derive(Debug, Clone)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TextEdit {
    /// Line number (1-based) where the edit starts
    pub line: u32,
    /// Column number (1-based) where the edit starts
    pub column: u32,
    /// Line number (1-based) where the edit ends
    pub end_line: u32,
    /// Column number (1-based) where the edit ends
    pub end_column: u32,
    /// The text to replace the range with
    pub new_text: String,
}

/// A type hint for a given symbol
#[derive(Debug, Clone)]
pub struct TypeHint {
    pub file_path: String,
    /// Line number (1-based) where the edit starts
    pub line: u32,
    /// Column number (1-based) where the edit starts
    pub column: u32,
    pub symbol: String,
    pub canonical_types: Vec<String>,
}

/// A completion item for a given cursor position
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// The primary name/identifier
    pub name: String,
    /// Alternative names (aliases)
    // pub aliases: Vec<String>,
    /// Required import
    pub required_import: Option<String>,
    /// The trait this method comes from (for trait methods)
    // pub trait_source: Option<String>,
    /// The kind of completion (function, variable, etc.)
    pub kind: Option<String>,
    /// The text to insert when this completion is selected
    // pub insert_text: String,
    /// Function signature or type information
    pub signature: Option<String>,
    /// Documentation for this completion
    pub documentation: Option<String>,
    /// Whether this completion is deprecated
    pub deprecated: bool,
}

/// Information about a reference location
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceInfo {
    /// Path to the file containing the reference
    pub file_path: String,
    /// Line number (1-based) where the reference starts
    pub line: u32,
    /// Column number (1-based) where the reference starts
    pub column: u32,
    /// Line number (1-based) where the reference ends
    pub end_line: u32,
    /// Column number (1-based) where the reference ends
    pub end_column: u32,
    /// Name of the referenced symbol
    pub name: String,
    /// Content of the reference (the line containing the reference)
    pub content: String,
    /// Whether this is a definition (true) or usage (false)
    pub is_definition: bool,
}

impl std::fmt::Display for TypeHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}\n```md\n{}\n```\nRelevant types: {}",
            self.file_path,
            self.line,
            self.column,
            self.symbol,
            self.canonical_types.join(", ")
        )
    }
}

impl std::fmt::Display for DefinitionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}\n{}",
            self.file_path, self.line, self.column, self.content
        )
    }
}

impl std::fmt::Display for RenameResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Successfully renamed symbol in {} file(s):",
            self.file_changes.len()
        )?;
        writeln!(f)?;
        for file_change in &self.file_changes {
            writeln!(f, "{file_change}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for FileChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.file_path)?;
        for edit in &self.edits {
            writeln!(f, "  ↳ {edit}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for TextEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}-{}:{} → '{}'",
            self.line, self.column, self.end_line, self.end_column, self.new_text
        )
    }
}

impl std::fmt::Display for CompletionItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ref kind) = self.kind {
            write!(f, " ({kind})")?;
        }
        if let Some(ref sig) = self.signature {
            write!(f, " - {sig}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for ReferenceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ref_type = if self.is_definition { "def" } else { "ref" };
        write!(
            f,
            "{}:{}:{} ({}) - {}",
            self.file_path,
            self.line,
            self.column,
            ref_type,
            self.content.trim()
        )
    }
}

/// Information about a code assist (code action)
#[derive(Debug, Clone)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct AssistInfo {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub target: String,
    pub source_change: Option<AssistSourceChange>,
}

impl std::fmt::Display for AssistInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}): {}", self.label, self.kind, self.target)
    }
}

/// Source change for an assist
#[derive(Debug, Clone)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct AssistSourceChange {
    pub file_changes: Vec<FileChange>,
    pub is_snippet: bool,
}

impl std::fmt::Display for AssistSourceChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Changes to {} files", self.file_changes.len())
    }
}

// --- New agent-native entity types ---

/// A single diagnostic fix with inline source changes
#[derive(Debug, Clone)]
pub struct DiagnosticFix {
    pub label: String,
    pub file_changes: Vec<FileChange>,
}

/// A diagnostic with optional quick-fixes
#[derive(Debug, Clone)]
pub struct DiagnosticInfo {
    pub message: String,
    pub severity: String,
    pub code: String,
    pub file_path: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub fixes: Vec<DiagnosticFix>,
}

impl fmt::Display for DiagnosticFix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "  fix: {}", self.label)?;
        for fc in &self.file_changes {
            write!(f, "\n    {fc}")?;
        }
        Ok(())
    }
}

impl fmt::Display for DiagnosticInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}:{}:{}: {} ({})",
            self.severity, self.file_path, self.line, self.column, self.message, self.code
        )?;
        for fix in &self.fixes {
            write!(f, "\n{fix}")?;
        }
        Ok(())
    }
}

/// Compound result for analyze_symbol — everything about a symbol in one call
#[derive(Debug, Clone)]
pub struct SymbolAnalysis {
    /// Type/hover info (markdown)
    pub type_info: Option<String>,
    /// Canonical type paths
    pub canonical_types: Vec<String>,
    /// Definition locations
    pub definitions: Vec<DefinitionInfo>,
    /// Implementations (trait implementors or implemented traits)
    pub implementations: Vec<DefinitionInfo>,
    /// Incoming callers (if it's a function)
    pub callers: Vec<CallerInfo>,
    /// Outgoing calls (if it's a function)
    pub callees: Vec<CallerInfo>,
    /// Number of references across the workspace
    pub reference_count: usize,
}

/// A caller/callee entry
#[derive(Debug, Clone)]
pub struct CallerInfo {
    pub name: String,
    pub file_path: String,
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for CallerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{} ({})",
            self.file_path, self.line, self.column, self.name
        )
    }
}

impl fmt::Display for SymbolAnalysis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref ti) = self.type_info {
            writeln!(f, "## Type Info\n{ti}")?;
        }
        if !self.canonical_types.is_empty() {
            writeln!(
                f,
                "\n## Canonical Types\n{}",
                self.canonical_types.join(", ")
            )?;
        }
        if !self.definitions.is_empty() {
            writeln!(f, "\n## Definition")?;
            for d in &self.definitions {
                writeln!(f, "{d}")?;
            }
        }
        if !self.implementations.is_empty() {
            writeln!(f, "\n## Implementations ({})", self.implementations.len())?;
            for imp in &self.implementations {
                writeln!(
                    f,
                    "- {}:{}:{} {}",
                    imp.file_path, imp.line, imp.column, imp.name
                )?;
            }
        }
        if !self.callers.is_empty() {
            writeln!(f, "\n## Callers ({})", self.callers.len())?;
            for c in &self.callers {
                writeln!(f, "- {c}")?;
            }
        }
        if !self.callees.is_empty() {
            writeln!(f, "\n## Callees ({})", self.callees.len())?;
            for c in &self.callees {
                writeln!(f, "- {c}")?;
            }
        }
        write!(f, "\n## References: {}", self.reference_count)?;
        Ok(())
    }
}

/// A file outline item (from file_structure)
#[derive(Debug, Clone)]
pub struct FileOutlineItem {
    pub name: String,
    pub kind: String,
    pub detail: Option<String>,
    pub line: u32,
    pub end_line: u32,
    pub parent_idx: Option<usize>,
    pub deprecated: bool,
}

impl fmt::Display for FileOutlineItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "L{}-{} {} {}",
            self.line, self.end_line, self.kind, self.name
        )?;
        if let Some(ref detail) = self.detail {
            write!(f, " — {detail}")?;
        }
        if self.deprecated {
            write!(f, " [deprecated]")?;
        }
        Ok(())
    }
}

/// A workspace symbol search result
#[derive(Debug, Clone)]
pub struct SymbolSearchResult {
    pub name: String,
    pub kind: Option<String>,
    pub file_path: String,
    pub line: u32,
    pub column: u32,
    pub container: Option<String>,
    pub description: Option<String>,
}

impl fmt::Display for SymbolSearchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{} {}",
            self.file_path, self.line, self.column, self.name
        )?;
        if let Some(ref kind) = self.kind {
            write!(f, " ({kind})")?;
        }
        if let Some(ref container) = self.container {
            write!(f, " in {container}")?;
        }
        if let Some(ref desc) = self.description {
            write!(f, " — {desc}")?;
        }
        Ok(())
    }
}

/// Macro expansion result
#[derive(Debug, Clone)]
pub struct MacroExpansion {
    pub name: String,
    pub expansion: String,
}

impl fmt::Display for MacroExpansion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Macro: {}\n\n```rust\n{}\n```",
            self.name, self.expansion
        )
    }
}

/// Function signature help
#[derive(Debug, Clone)]
pub struct SignatureInfo {
    pub signature: String,
    pub parameters: Vec<String>,
    pub active_parameter: Option<usize>,
    pub documentation: Option<String>,
}

impl fmt::Display for SignatureInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.signature)?;
        if !self.parameters.is_empty() {
            writeln!(f, "\nParameters:")?;
            for (i, param) in self.parameters.iter().enumerate() {
                let marker = if self.active_parameter == Some(i) {
                    " →"
                } else {
                    "  "
                };
                writeln!(f, "{marker} {param}")?;
            }
        }
        if let Some(ref doc) = self.documentation {
            write!(f, "\n{doc}")?;
        }
        Ok(())
    }
}
