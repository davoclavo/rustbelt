//! Rust-Analyzer Integration Module
//!
//! This module provides a wrapper around rust-analyzer's IDE functionality,
//! making it easy to get type hints, definitions, and other semantic
//! information.

use std::path::PathBuf;

use anyhow::Result;
use ra_ap_hir::ClosureStyle;
use ra_ap_ide::{
    AdjustmentHints, AdjustmentHintsMode, Analysis, AnalysisHost, CallableSnippets,
    ClosureReturnTypeHints, CompletionConfig, CompletionFieldsToResolve,
    CompletionItemKind as RaCompletionItemKind, DiscriminantHints, FileId, FilePosition, FileRange,
    FindAllRefsConfig, GenericParameterHints, GotoDefinitionConfig, HoverConfig, HoverDocFormat,
    InlayFieldsToResolve, InlayHintPosition, InlayHintsConfig, LifetimeElisionHints, LineCol,
    LineIndex, MonikerResult, RenameConfig, SubstTyLen, TextRange, TextSize,
};
use ra_ap_ide_assists::{AssistConfig, AssistResolveStrategy, assists};
use ra_ap_ide_db::MiniCore;
use ra_ap_ide_db::imports::insert_use::{ImportGranularity, InsertUseConfig, PrefixKind};
use ra_ap_ide_db::text_edit::TextEditBuilder;
use tracing::{debug, trace, warn};

use super::entities::{
    AssistInfo, AssistSourceChange, CompletionItem, CursorCoordinates, DefinitionInfo, FileChange,
    ReferenceInfo, RenameResult, TextEdit, TypeHint,
};
use super::file_watcher::FileWatcher;
use super::utils::RustAnalyzerUtils;

/// Main interface to rust-analyzer functionality
///
/// This struct provides semantic analysis capabilities for Rust code, including:
/// - Type hints and definitions
/// - Code completion
/// - Symbol renaming and references
/// - File watching for automatic updates
///
/// Use RustAnalyzerishBuilder to create properly configured instances.
#[derive(Debug)]
pub struct RustAnalyzerish {
    host: AnalysisHost,
    file_watcher: FileWatcher,
}

impl RustAnalyzerish {
    /// Create a new RustAnalyzer instance with a loaded workspace
    ///
    /// This is called by RustAnalyzerishBuilder after workspace loading.
    pub fn new(host: AnalysisHost, file_watcher: FileWatcher) -> Self {
        Self { host, file_watcher }
    }

    /// Debug information about the current cursor position
    ///
    /// # Arguments
    ///
    /// * `cursor` - The cursor coordinates to debug
    /// * `file_id` - The file ID for the file
    /// * `offset` - The text offset within the file
    /// * `analysis` - The analysis instance for reading file content
    fn debug_cursor_position(
        &self,
        cursor: &CursorCoordinates,
        file_id: FileId,
        offset: TextSize,
        analysis: &Analysis,
    ) {
        debug!(
            "Cursor position: file={:?}, line={}, column={}, offset={:?}",
            file_id, cursor.line, cursor.column, offset
        );

        // Debug the current character at the offset
        if let Ok(source_text) = analysis.file_text(file_id) {
            let offset_usize: usize = offset.into();
            if offset_usize < source_text.len() {
                let current_char = source_text[offset_usize..].chars().next().unwrap_or('?');
                debug!(
                    "Current character at {}:{} (offset {:?}): '{}'",
                    cursor.line, cursor.column, offset, current_char
                );

                // Show context around the cursor (5 chars before and after)
                let start = offset_usize.saturating_sub(5);
                let end = (offset_usize + 5).min(source_text.len());
                let context = &source_text[start..end];
                let cursor_pos = offset_usize - start;
                debug!(
                    "Context around cursor: '{}' (cursor at position {})",
                    context.replace('\n', "\\n").replace('\t', "\\t"),
                    cursor_pos
                );
            } else {
                debug!(
                    "Offset {:?} is out of bounds for file text length {}",
                    offset,
                    source_text.len()
                );
            }
        } else {
            debug!("Failed to read source text for file ID {:?}", file_id);
        }
    }

    /// Validate cursor coordinates and convert to text offset
    ///
    /// # Arguments
    ///
    /// * `cursor` - The cursor coordinates to validate (must be 1-based)
    /// * `line_index` - The line index for the file to validate against
    ///
    /// # Errors
    ///
    /// Returns an error if coordinates are invalid (0 or out of bounds)
    fn validate_and_convert_cursor(
        &self,
        cursor: &CursorCoordinates,
        line_index: &LineIndex,
    ) -> Result<TextSize> {
        // Validate coordinates before proceeding
        if cursor.line == 0 || cursor.column == 0 {
            return Err(anyhow::anyhow!(
                "Invalid coordinates in file '{}': line and column must be >= 1, got {}:{}",
                cursor.file_path,
                cursor.line,
                cursor.column
            ));
        }

        // Convert line/column to text offset from 1-based to 0-based indexing
        let line_col: LineCol = cursor.into();
        line_index.offset(line_col).ok_or_else(|| {
            anyhow::anyhow!(
                "Coordinates out of bounds in file '{}': {}:{} (file may have changed)",
                cursor.file_path,
                cursor.line,
                cursor.column
            )
        })
    }

    /// Common setup for cursor-based operations
    ///
    /// Prepares analysis, validates cursor, and returns common data
    async fn setup_cursor_analysis(
        &mut self,
        raw_cursor: &CursorCoordinates,
    ) -> Result<(Analysis, FileId, TextSize, CursorCoordinates)> {
        // Ensure file watcher changes are applied
        self.file_watcher.drain_and_apply_changes(&mut self.host)?;

        let analysis = self.host.analysis();
        let file_id = self
            .file_watcher
            .get_file_id(&PathBuf::from(&raw_cursor.file_path))?;

        // Resolve coordinates if a symbol is provided
        let resolved_cursor = if raw_cursor.symbol.is_some() {
            // Get file content for symbol resolution
            let file_content = std::fs::read_to_string(&raw_cursor.file_path)
                .map_err(|e| anyhow::anyhow!("Failed to read file content: {}", e))?;
            raw_cursor.resolve_coordinates(&file_content)
        } else {
            raw_cursor.clone()
        };

        // Get the file's line index for position conversion
        let line_index = analysis.file_line_index(file_id).map_err(|_| {
            anyhow::anyhow!(
                "Failed to get line index for file: {}",
                raw_cursor.file_path
            )
        })?;

        // Validate and convert cursor coordinates (using resolved coordinates)
        let offset = self.validate_and_convert_cursor(&resolved_cursor, &line_index)?;

        // Debug cursor position (show both original and resolved if different)
        if let Some(symbol) = raw_cursor.symbol.as_ref()
            && (raw_cursor.line != resolved_cursor.line
                || raw_cursor.column != resolved_cursor.column)
        {
            trace!(
                "Symbol '{}' resolved from {}:{} to {}:{}",
                symbol,
                raw_cursor.line,
                raw_cursor.column,
                resolved_cursor.line,
                resolved_cursor.column
            );
        }
        self.debug_cursor_position(&resolved_cursor, file_id, offset, &analysis);

        Ok((analysis, file_id, offset, resolved_cursor))
    }

    /// Create a FilePosition from file_id and offset
    fn create_file_position(file_id: FileId, offset: TextSize) -> FilePosition {
        FilePosition { file_id, offset }
    }

    /// Get type hint information at the specified cursor position
    pub async fn get_type_hint(
        &mut self,
        raw_cursor: &CursorCoordinates,
    ) -> Result<Option<TypeHint>> {
        let (analysis, file_id, offset, cursor) = self.setup_cursor_analysis(raw_cursor).await?;

        // Create TextRange for the hover query - use a single point range
        let text_range = TextRange::new(offset, offset);

        let hover_config = HoverConfig {
            links_in_hover: true,
            memory_layout: None,
            documentation: true,
            keywords: true,
            // TODO Consider using Markdown but figure out how to reliably show symbol names too
            format: HoverDocFormat::PlainText,
            max_trait_assoc_items_count: Some(10),
            max_fields_count: Some(10),
            max_enum_variants_count: Some(10),
            max_subst_ty_len: SubstTyLen::Unlimited,
            show_drop_glue: false,
            minicore: MiniCore::default(),
        };

        debug!(
            "Attempting hover query for file {:?} at offset {:?} (line {} col {})",
            file_id, offset, cursor.line, cursor.column
        );

        // Try hover with the configured settings
        let hover_result = match analysis.hover(
            &hover_config,
            FileRange {
                file_id,
                range: text_range,
            },
        ) {
            Ok(Some(result)) => result,
            Ok(None) => {
                debug!(
                    "No hover info available for {}:{}:{}",
                    cursor.file_path, cursor.line, cursor.column
                );
                return Ok(None);
            }
            Err(e) => {
                warn!("Hover analysis failed: {:?}", e);
                return Err(anyhow::anyhow!("Hover analysis failed: {:?}", e));
            }
        };

        trace!(
            "Hover result for {}:{}:{}: {:?}",
            cursor.file_path, cursor.line, cursor.column, hover_result
        );
        // Get the type information from hover
        let mut canonical_types: Vec<String> = Vec::new();
        for action in hover_result.info.actions {
            match action {
                ra_ap_ide::HoverAction::GoToType(type_actions) => {
                    for type_action in type_actions {
                        canonical_types.push(type_action.mod_path);
                    }
                }
                _ => debug!("Unhandled hover action: {:?}", action),
            }
        }

        debug!(
            "Got type hint for {}:{}:{}",
            cursor.file_path, cursor.line, cursor.column
        );

        let type_hint = TypeHint {
            file_path: cursor.file_path.clone(),
            line: cursor.line,
            column: cursor.column,
            symbol: hover_result.info.markup.to_string(),
            canonical_types,
        };

        Ok(Some(type_hint))
    }

    /// Get completion suggestions at the specified cursor position
    pub async fn get_completions(
        &mut self,
        raw_cursor: &CursorCoordinates,
    ) -> Result<Option<Vec<CompletionItem>>> {
        let (analysis, file_id, offset, cursor) = self.setup_cursor_analysis(raw_cursor).await?;

        debug!(
            "Attempting completions query for file {:?} at offset {:?} (line {} col {})",
            file_id, offset, cursor.line, cursor.column
        );

        let position = Self::create_file_position(file_id, offset);

        let config = CompletionConfig {
            enable_postfix_completions: true,
            enable_imports_on_the_fly: false, // Keep simple for now
            enable_self_on_the_fly: false,
            enable_auto_iter: true,
            enable_auto_await: true,
            enable_private_editable: false,
            enable_term_search: false,
            term_search_fuel: 400,
            full_function_signatures: false,
            callable: Some(CallableSnippets::FillArguments),
            add_semicolon_to_unit: false,
            snippet_cap: None, // Disable snippets for simplicity
            insert_use: InsertUseConfig {
                granularity: ImportGranularity::Crate,
                enforce_granularity: true,
                prefix_kind: PrefixKind::Plain,
                group: true,
                skip_glob_imports: true,
            },
            prefer_no_std: false,
            prefer_prelude: true,
            prefer_absolute: false,
            snippets: vec![],
            limit: Some(200), // Limit results for performance
            fields_to_resolve: CompletionFieldsToResolve::empty(),
            exclude_flyimport: vec![],
            exclude_traits: &[],
            minicore: MiniCore::default(),
        };

        match analysis.completions(&config, position, Some('.')) {
            Ok(Some(ra_completions)) => {
                let mut completions = Vec::new();

                for completion_item in ra_completions {
                    // Convert rust-analyzer CompletionItem to our CompletionItem
                    let kind = match completion_item.kind {
                        RaCompletionItemKind::SymbolKind(symbol_kind) => {
                            Some(format!("{:?}", symbol_kind))
                        }
                        RaCompletionItemKind::Binding => Some("Binding".to_string()),
                        RaCompletionItemKind::BuiltinType => Some("BuiltinType".to_string()),
                        RaCompletionItemKind::InferredType => Some("InferredType".to_string()),
                        RaCompletionItemKind::Keyword => Some("Keyword".to_string()),
                        RaCompletionItemKind::Snippet => Some("Snippet".to_string()),
                        RaCompletionItemKind::UnresolvedReference => {
                            Some("UnresolvedReference".to_string())
                        }
                        RaCompletionItemKind::Expression => Some("Expression".to_string()),
                    };

                    let documentation = completion_item
                        .documentation
                        .map(|doc| doc.as_str().to_string());

                    // TODO Consider label left/right details
                    let name = completion_item.label.primary.into();
                    let required_import = if completion_item.import_to_add.is_empty() {
                        None
                    } else {
                        Some(completion_item.import_to_add.join(", "))
                    };

                    let completion = CompletionItem {
                        name,
                        required_import,
                        kind,
                        signature: completion_item.detail,
                        documentation,
                        deprecated: completion_item.deprecated,
                    };

                    completions.push(completion);
                }

                debug!(
                    "Found {} completions for {}:{}:{}",
                    completions.len(),
                    cursor.file_path,
                    cursor.line,
                    cursor.column
                );

                Ok(Some(completions))
            }
            Ok(None) => {
                debug!(
                    "No completions available for {}:{}:{}",
                    cursor.file_path, cursor.line, cursor.column
                );
                Ok(None)
            }
            Err(e) => {
                warn!("Completion analysis failed: {:?}", e);
                Err(anyhow::anyhow!("Completion analysis failed: {:?}", e))
            }
        }
    }

    /// Get definition information at the specified cursor position
    pub async fn get_definition(
        &mut self,
        raw_cursor: &CursorCoordinates,
    ) -> Result<Option<Vec<DefinitionInfo>>> {
        let (analysis, file_id, offset, cursor) = self.setup_cursor_analysis(raw_cursor).await?;

        debug!(
            "Attempting goto_definition query for file {:?} at offset {:?} (line {} col {})",
            file_id, offset, cursor.line, cursor.column
        );

        // Query for definitions
        // Use std::panic::catch_unwind to handle potential panics in rust-analyzer
        // Happens when we query colum: 1 row: 1
        // TODO Report bug
        let goto_config = GotoDefinitionConfig {
            minicore: MiniCore::default(),
        };
        let goto_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            analysis.goto_definition(Self::create_file_position(file_id, offset), &goto_config)
        }));

        let definitions_result = match goto_result {
            Ok(result) => result,
            Err(_panic) => {
                debug!(
                    "Caught panic during goto_definition for {}:{}:{}, likely due to edge case in rust-analyzer",
                    cursor.file_path, cursor.line, cursor.column
                );
                return Ok(None);
            }
        };

        match definitions_result {
            Ok(Some(range_info)) => {
                let mut definitions = Vec::new();

                for nav in range_info.info {
                    debug!("Navigation target: {:?}", nav);
                    // Get file path from file_id
                    if let Ok(line_index) = analysis.file_line_index(nav.file_id) {
                        let start_line_col = line_index.line_col(nav.focus_or_full_range().start());
                        let end_line_col = line_index.line_col(nav.focus_or_full_range().end());

                        let file_path = {
                            if let Some(path) = self.file_watcher.file_path(nav.file_id) {
                                path
                            } else {
                                return Err(anyhow::anyhow!(
                                    "File ID {:?} not found in VFS",
                                    &nav.file_id
                                ));
                            }
                        };

                        // Get module path using moniker if available
                        let module = if let Ok(Some(moniker_info)) =
                            analysis.moniker(FilePosition {
                                file_id: nav.file_id,
                                offset: nav.focus_or_full_range().start(),
                            }) {
                            // Extract module path from moniker
                            match &moniker_info.info.first() {
                                Some(MonikerResult::Moniker(moniker)) => {
                                    // Build full module path from crate name and description
                                    let crate_name = &moniker.identifier.crate_name;
                                    let module_parts: Vec<String> = moniker
                                        .identifier
                                        .description
                                        .iter()
                                        .map(|desc| desc.name.to_string())
                                        .collect();

                                    if module_parts.is_empty() {
                                        crate_name.clone()
                                    } else {
                                        format!("{}::{}", crate_name, module_parts.join("::"))
                                    }
                                }
                                Some(MonikerResult::Local { .. }) => {
                                    // For local symbols, fall back to container name
                                    nav.container_name
                                        .as_ref()
                                        .map(|name| name.to_string())
                                        .unwrap_or_else(|| "local".to_string())
                                }
                                None => {
                                    // Fall back to container name
                                    nav.container_name
                                        .as_ref()
                                        .map(|name| name.to_string())
                                        .unwrap_or_else(|| "unknown".to_string())
                                }
                            }
                        } else {
                            // Fall back to container name if moniker fails
                            nav.container_name
                                .as_ref()
                                .map(|name| name.to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        };

                        // Extract definition content from source
                        let content = if let Ok(source_text) = analysis.file_text(nav.file_id) {
                            let full_range = nav.full_range;
                            let start_offset = full_range.start().into();
                            let end_offset = full_range.end().into();

                            if start_offset < source_text.len() && end_offset <= source_text.len() {
                                source_text[start_offset..end_offset].to_string()
                            } else {
                                format!(
                                    "// Content extraction failed: invalid range {start_offset}..{end_offset}"
                                )
                            }
                        } else {
                            "// Content extraction failed: could not read source".to_string()
                        };

                        let definition = DefinitionInfo {
                            file_path,
                            line: start_line_col.line + 1, // Convert back to 1-based
                            column: start_line_col.col + 1, // Convert back to 1-based
                            end_line: end_line_col.line + 1,
                            end_column: end_line_col.col + 1,
                            name: nav.name.to_string(),
                            kind: nav.kind,
                            description: nav.description.clone(),
                            module,
                            content,
                        };
                        debug!("Found definition: {:?}", definition);
                        definitions.push(definition);
                    }
                }

                debug!(
                    "Found {} definitions for {}:{}:{}",
                    definitions.len(),
                    cursor.file_path,
                    cursor.line,
                    cursor.column
                );
                Ok(Some(definitions))
            }
            Ok(None) => {
                debug!(
                    "No definitions available for {}:{}:{}",
                    cursor.file_path, cursor.line, cursor.column
                );
                Ok(None)
            }
            Err(e) => {
                warn!("Goto definition analysis failed: {:?}", e);
                Err(anyhow::anyhow!("Goto definition analysis failed: {:?}", e))
            }
        }
    }

    /// Rename a symbol at the specified cursor position and apply the changes
    /// to disk
    pub async fn rename_symbol(
        &mut self,
        raw_cursor: &CursorCoordinates,
        new_name: &str,
    ) -> Result<Option<RenameResult>> {
        // Get the rename information
        let rename_result = self.get_rename_info(raw_cursor, new_name).await?;

        if let Some(ref result) = rename_result {
            // Apply the edits to disk
            RustAnalyzerUtils::apply_rename_edits(result).await?;
        }

        Ok(rename_result)
    }

    /// Find all references to a symbol at the specified cursor position
    pub async fn find_references(
        &mut self,
        raw_cursor: &CursorCoordinates,
    ) -> Result<Option<Vec<ReferenceInfo>>> {
        let (analysis, file_id, offset, cursor) = self.setup_cursor_analysis(raw_cursor).await?;

        debug!(
            "Attempting find_all_refs query for file {:?} at offset {:?} (line {} col {})",
            file_id, offset, cursor.line, cursor.column
        );

        // Query for all references
        let find_refs_config = FindAllRefsConfig {
            search_scope: None,
            minicore: MiniCore::default(),
        };
        let references_result = match analysis.find_all_refs(
            Self::create_file_position(file_id, offset),
            &find_refs_config,
        ) {
            Ok(Some(search_results)) => search_results,
            Ok(None) => {
                debug!("No references found at position");
                return Ok(None);
            }
            Err(e) => {
                debug!("Error finding references: {}", e);
                return Err(anyhow::anyhow!("Failed to find references: {}", e));
            }
        };

        let mut references = Vec::new();

        for search_result in references_result {
            // Add the declaration (definition) if it exists
            if let Some(declaration) = &search_result.declaration
                && let Ok(decl_line_index) = analysis.file_line_index(declaration.nav.file_id)
            {
                let decl_range = declaration.nav.focus_or_full_range();
                let start_line_col = decl_line_index.line_col(decl_range.start());
                let end_line_col = decl_line_index.line_col(decl_range.end());

                if let Some(decl_file_path) = self.file_watcher.file_path(declaration.nav.file_id) {
                    // Get the line content containing the declaration
                    let content = if let Ok(file_text) = analysis.file_text(declaration.nav.file_id)
                    {
                        Self::get_line_content(&file_text, start_line_col.line as usize)
                    } else {
                        "".to_string()
                    };

                    references.push(ReferenceInfo {
                        file_path: decl_file_path,
                        line: start_line_col.line + 1,
                        column: start_line_col.col + 1,
                        end_line: end_line_col.line + 1,
                        end_column: end_line_col.col + 1,
                        name: declaration.nav.name.to_string(),
                        content,
                        is_definition: true,
                    });
                }
            }

            // Process all references grouped by file
            for (ref_file_id, ref_ranges) in search_result.references {
                if let Ok(ref_line_index) = analysis.file_line_index(ref_file_id)
                    && let Some(ref_file_path) = self.file_watcher.file_path(ref_file_id)
                {
                    // Get file text once for this file
                    if let Ok(file_text) = analysis.file_text(ref_file_id) {
                        let symbol_name = search_result
                            .declaration
                            .as_ref()
                            .map(|d| d.nav.name.to_string())
                            .unwrap_or_else(|| "unknown".to_string());

                        // Process each reference range in this file
                        for (range, _category) in ref_ranges {
                            let start_line_col = ref_line_index.line_col(range.start());
                            let end_line_col = ref_line_index.line_col(range.end());

                            let content =
                                Self::get_line_content(&file_text, start_line_col.line as usize);

                            references.push(ReferenceInfo {
                                file_path: ref_file_path.clone(),
                                line: start_line_col.line + 1,
                                column: start_line_col.col + 1,
                                end_line: end_line_col.line + 1,
                                end_column: end_line_col.col + 1,
                                name: symbol_name.clone(),
                                content,
                                is_definition: false,
                            });
                        }
                    }
                }
            }
        }

        if references.is_empty() {
            return Err(anyhow::anyhow!("No references or declarations found"));
        }

        // Sort references by file path, then by line number
        references.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.line.cmp(&b.line))
                .then_with(|| a.column.cmp(&b.column))
        });
        Ok(Some(references))
    }

    /// Helper method to get line content from file text
    fn get_line_content(file_text: &str, line_number: usize) -> String {
        RustAnalyzerUtils::get_line_content(file_text, line_number).unwrap_or_default()
    }

    /// Get rename information without applying changes to disk
    pub async fn get_rename_info(
        &mut self,
        raw_cursor: &CursorCoordinates,
        new_name: &str,
    ) -> Result<Option<RenameResult>> {
        let (analysis, file_id, offset, cursor) = self.setup_cursor_analysis(raw_cursor).await?;

        debug!(
            "Attempting rename for file {:?} at offset {:?} (line {} col {}) to '{}'",
            file_id, offset, cursor.line, cursor.column, new_name
        );

        let position = Self::create_file_position(file_id, offset);

        // TODO Consider separating this to a separate tool
        // First, prepare the rename to validate it's possible
        // let prepare_result = match analysis.prepare_rename(position) {
        //     Ok(result) => result,
        //     Err(e) => {
        //         warn!("Failed to prepare rename: {:?}", e);
        //         bail!("Failed to prepare rename: {:?}", e)
        //     }
        // };

        // let _prepare_range_info = match prepare_result {
        //     Ok(range_info) => range_info,
        //     Err(rename_error) => {
        //         debug!("Rename not possible: {:?}", rename_error);
        //         return Ok(None);
        //     }
        // };

        // Perform the actual rename
        let rename_config = RenameConfig {
            prefer_no_std: false,
            prefer_prelude: true,
            prefer_absolute: false,
            show_conflicts: true,
        };
        let rename_result = match analysis.rename(position, new_name, &rename_config) {
            Ok(result) => result,
            Err(e) => {
                warn!("Failed to perform rename: {:?}", e);
                return Err(anyhow::anyhow!("Failed to perform rename: {:?}", e));
            }
        };

        let source_change = match rename_result {
            Ok(source_change) => source_change,
            Err(rename_error) => {
                debug!("Rename failed: {:?}", rename_error);
                return Ok(None);
            }
        };

        // Convert SourceChange to our RenameResult format
        let mut file_changes = Vec::new();

        for (file_id, edit_tuple) in source_change.source_file_edits {
            // Get file path from file_id
            let file_path = {
                if let Some(path) = self.file_watcher.file_path(file_id) {
                    path
                } else {
                    return Err(anyhow::anyhow!("File ID {:?} not found in VFS", file_id));
                }
            };

            // Get line index for this file
            let file_line_index = analysis
                .file_line_index(file_id)
                .map_err(|_| anyhow::anyhow!("Failed to get line index for file {:?}", file_id))?;

            // Convert text edits - the tuple is (TextEdit, Option<SnippetEdit>)
            let mut edits = Vec::new();
            let text_edit = &edit_tuple.0; // Get the TextEdit from the tuple

            for edit in text_edit.iter() {
                let start_line_col = file_line_index.line_col(edit.delete.start());
                let end_line_col = file_line_index.line_col(edit.delete.end());

                edits.push(TextEdit {
                    line: start_line_col.line + 1,  // Convert to 1-based
                    column: start_line_col.col + 1, // Convert to 1-based
                    end_line: end_line_col.line + 1,
                    end_column: end_line_col.col + 1,
                    new_text: edit.insert.clone(),
                });
            }

            file_changes.push(FileChange { file_path, edits });
        }

        debug!(
            "Rename successful: {} file(s) will be changed",
            file_changes.len()
        );

        Ok(Some(RenameResult { file_changes }))
    }

    /// View a Rust file with inlay hints
    pub async fn view_inlay_hints(
        &mut self,
        file_path: &str,
        start_line: Option<u32>,
        end_line: Option<u32>,
    ) -> Result<String> {
        let path = PathBuf::from(file_path);

        // Ensure file watcher changes are applied
        self.file_watcher.drain_and_apply_changes(&mut self.host)?;

        let analysis = self.host.analysis();
        let file_id = self.file_watcher.get_file_id(&path)?;

        // Get the file content
        let file_content = analysis
            .file_text(file_id)
            .map_err(|_| anyhow::anyhow!("Failed to get file content for: {}", file_path))?;

        // Configure inlay hints to show type information
        let inlay_config = InlayHintsConfig {
            render_colons: false,
            type_hints: true,
            sized_bound: false,
            discriminant_hints: DiscriminantHints::Never,
            parameter_hints: true,
            parameter_hints_for_missing_arguments: false,
            generic_parameter_hints: GenericParameterHints {
                type_hints: false,
                lifetime_hints: false,
                const_hints: false,
            },
            chaining_hints: false,
            adjustment_hints: AdjustmentHints::Never,
            adjustment_hints_mode: AdjustmentHintsMode::Prefix,
            adjustment_hints_hide_outside_unsafe: false,
            adjustment_hints_disable_reborrows: false,
            closure_return_type_hints: ClosureReturnTypeHints::Never,
            closure_capture_hints: false,
            binding_mode_hints: false,
            implicit_drop_hints: false,
            lifetime_elision_hints: LifetimeElisionHints::Never,
            param_names_for_lifetime_elision_hints: false,
            hide_named_constructor_hints: false,
            hide_closure_initialization_hints: false,
            hide_closure_parameter_hints: false,
            hide_inferred_type_hints: false,
            implied_dyn_trait_hints: false,
            range_exclusive_hints: false,
            closure_style: ClosureStyle::ImplFn,
            max_length: None,
            closing_brace_hints_min_lines: None,
            fields_to_resolve: InlayFieldsToResolve {
                resolve_text_edits: false,
                resolve_hint_tooltip: false,
                resolve_label_tooltip: false,
                resolve_label_location: false,
                resolve_label_command: false,
            },
            minicore: MiniCore::default(),
        };

        // Get inlay hints for the entire file
        let inlay_hints = analysis
            .inlay_hints(&inlay_config, file_id, None)
            .map_err(|_| anyhow::anyhow!("Failed to get inlay hints for file: {}", file_path))?;

        debug!(
            "Found {} inlay hints for file: {}",
            inlay_hints.len(),
            file_path
        );

        // Use TextEditBuilder to apply all inlay hints as insertions
        let mut builder = TextEditBuilder::default();

        for hint in inlay_hints {
            // Create the type annotation text
            let hint_text = hint
                .label
                .parts
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join("");

            let (offset, full_hint_text) = match hint.position {
                InlayHintPosition::After => (hint.range.end(), format!(": {}", hint_text)),
                InlayHintPosition::Before => (hint.range.start(), format!("{}: ", hint_text)),
            };

            trace!("Inlay hint at offset {:?}: {:?}", offset, hint);

            // Insert the annotation at the correct position
            builder.insert(offset, full_hint_text);
        }

        // Apply all edits to the content
        let text_edit = builder.finish();
        let mut result = file_content.to_string();
        text_edit.apply(&mut result);

        // If line range was specified, extract only that range from the result
        if let (Some(start), Some(end)) = (start_line, end_line) {
            let lines: Vec<&str> = result.lines().collect();
            let start_idx = (start.saturating_sub(1) as usize).min(lines.len());
            let end_idx = (end as usize).min(lines.len());

            if start_idx >= lines.len() || end_idx <= start_idx {
                return Err(anyhow::anyhow!("Range outside of the file limits"));
            }

            let selected_lines = &lines[start_idx..end_idx];
            Ok(selected_lines.join("\n"))
        } else {
            Ok(result)
        }
    }

    /// Get available code assists at the specified cursor position
    pub async fn get_assists(
        &mut self,
        raw_cursor: &CursorCoordinates,
    ) -> Result<Option<Vec<AssistInfo>>> {
        let cursor = raw_cursor.resolve_coordinates(
            &std::fs::read_to_string(&raw_cursor.file_path).unwrap_or_default(),
        );

        self.file_watcher.drain_and_apply_changes(&mut self.host)?;

        let path = PathBuf::from(&cursor.file_path);
        let file_id = self.file_watcher.get_file_id(&path)?;

        let analysis = self.host.analysis();

        // Convert 1-based line/column to 0-based for rust-analyzer
        let line_col = LineCol {
            line: cursor.line.saturating_sub(1),
            col: cursor.column.saturating_sub(1),
        };

        // Get the line index and convert to TextSize offset
        let line_index = analysis
            .file_line_index(file_id)
            .map_err(|_| anyhow::anyhow!("Failed to get line index"))?;

        let offset = line_index.offset(line_col).unwrap_or(TextSize::from(0));

        self.debug_cursor_position(&cursor, file_id, offset, &analysis);

        let file_range = FileRange {
            file_id,
            range: TextRange::new(offset, offset),
        };

        // Create assist config with reasonable defaults
        let assist_config = AssistConfig {
            snippet_cap: None,
            allowed: None,
            insert_use: InsertUseConfig {
                granularity: ImportGranularity::Crate,
                enforce_granularity: true,
                prefix_kind: PrefixKind::Plain,
                group: true,
                skip_glob_imports: true,
            },
            prefer_no_std: false,
            prefer_prelude: false,
            prefer_absolute: false,
            assist_emit_must_use: false,
            term_search_fuel: 400,
            term_search_borrowck: true,
            code_action_grouping: false,
            expr_fill_default: ra_ap_ide_db::assists::ExprFillDefaultMode::Todo,
            prefer_self_ty: false,
            show_rename_conflicts: true,
        };

        // Get available assists
        let assists_result = assists(
            self.host.raw_database(),
            &assist_config,
            AssistResolveStrategy::None,
            file_range,
        );

        if assists_result.is_empty() {
            Ok(None)
        } else {
            let assist_infos = assists_result
                .into_iter()
                .map(|assist| AssistInfo {
                    id: assist.id.0.to_string(),
                    kind: if let Some(group) = &assist.group {
                        group.0.to_string()
                    } else {
                        "refactor".to_string()
                    },
                    label: assist.label.to_string(),
                    target: format!("{:?}", assist.target),
                    source_change: None,
                })
                .collect();

            Ok(Some(assist_infos))
        }
    }

    /// Apply a specific code assist at the specified cursor position
    pub async fn apply_assist(
        &mut self,
        raw_cursor: &CursorCoordinates,
        assist_id: &str,
    ) -> Result<Option<AssistSourceChange>> {
        let cursor = raw_cursor.resolve_coordinates(
            &std::fs::read_to_string(&raw_cursor.file_path).unwrap_or_default(),
        );

        self.file_watcher.drain_and_apply_changes(&mut self.host)?;

        let path = PathBuf::from(&cursor.file_path);
        let file_id = self.file_watcher.get_file_id(&path)?;

        let analysis = self.host.analysis();

        // Convert 1-based line/column to 0-based for rust-analyzer
        let line_col = LineCol {
            line: cursor.line.saturating_sub(1),
            col: cursor.column.saturating_sub(1),
        };

        // Get the line index and convert to TextSize offset
        let line_index = analysis
            .file_line_index(file_id)
            .map_err(|_| anyhow::anyhow!("Failed to get line index"))?;

        let offset = line_index.offset(line_col).unwrap_or(TextSize::from(0));

        self.debug_cursor_position(&cursor, file_id, offset, &analysis);

        let file_range = FileRange {
            file_id,
            range: TextRange::new(offset, offset),
        };

        // Create assist config with reasonable defaults
        let assist_config = AssistConfig {
            snippet_cap: None,
            allowed: None,
            insert_use: InsertUseConfig {
                granularity: ImportGranularity::Crate,
                enforce_granularity: true,
                prefix_kind: PrefixKind::Plain,
                group: true,
                skip_glob_imports: true,
            },
            prefer_no_std: false,
            prefer_prelude: false,
            prefer_absolute: false,
            assist_emit_must_use: false,
            term_search_fuel: 400,
            term_search_borrowck: true,
            code_action_grouping: false,
            expr_fill_default: ra_ap_ide_db::assists::ExprFillDefaultMode::Todo,
            prefer_self_ty: false,
            show_rename_conflicts: true,
        };

        // Get available assists with resolved source changes
        let assists_result = assists(
            self.host.raw_database(),
            &assist_config,
            AssistResolveStrategy::All,
            file_range,
        );

        // Find the specific assist by ID
        let target_assist = assists_result
            .into_iter()
            .find(|assist| assist.id.0 == assist_id);

        if let Some(assist) = target_assist {
            if let Some(source_change) = assist.source_change {
                // Convert rust-analyzer source change to our format
                let file_changes = source_change
                    .source_file_edits
                    .into_iter()
                    .map(|(file_id, (text_edit, _snippet_edit))| {
                        let file_path = self
                            .file_watcher
                            .file_path(file_id)
                            .unwrap_or_else(|| "unknown".to_string());

                        let edits = text_edit
                            .into_iter()
                            .map(|indel| {
                                let line_index = analysis.file_line_index(file_id).unwrap();
                                let start_line_col = line_index.line_col(indel.delete.start());
                                let end_line_col = line_index.line_col(indel.delete.end());

                                TextEdit {
                                    line: start_line_col.line + 1,
                                    column: start_line_col.col + 1,
                                    end_line: end_line_col.line + 1,
                                    end_column: end_line_col.col + 1,
                                    new_text: indel.insert,
                                }
                            })
                            .collect();

                        FileChange { file_path, edits }
                    })
                    .collect();

                // Apply the changes to disk
                for file_change in &file_changes {
                    RustAnalyzerUtils::apply_file_change(file_change).await?;
                }

                let assist_source_change = AssistSourceChange {
                    file_changes,
                    is_snippet: source_change.is_snippet,
                };

                Ok(Some(assist_source_change))
            } else {
                Err(anyhow::anyhow!("Assist has no source change available"))
            }
        } else {
            Ok(None)
        }
    }
}
