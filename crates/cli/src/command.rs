use anyhow::Result;
use clap::{Parser, Subcommand};
use librustbelt::{
    analyzer::RustAnalyzerish, builder::RustAnalyzerishBuilder, entities::CursorCoordinates,
};

// Unified command wrapper for both CLI and REPL use
#[derive(Parser)]
#[command(no_binary_name = true)]
pub struct CommandWrapper {
    #[command(subcommand)]
    pub command: AnalyzerCommand,
}

// Base commands without workspace path - used by both CLI and REPL
#[derive(Subcommand)]
#[command(no_binary_name = true)]
pub enum AnalyzerCommand {
    /// Get type hint for a specific position
    TypeHint {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Get definition details for a symbol at a specific position
    GetDefinition {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Get completion suggestions at a specific position
    GetCompletions {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Find all references to a symbol at a specific position
    FindReferences {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// View a Rust file with embedded inlay hints such as types and named arguments
    ViewInlayHints {
        /// Path to the Rust source file
        file_path: String,
        /// Starting line number (1-based, optional)
        #[arg(long)]
        start_line: Option<u32>,
        /// Ending line number (1-based, optional)
        #[arg(long)]
        end_line: Option<u32>,
    },

    /// Get available code assists (code actions) at a specific position
    GetAssists {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Apply a specific code assist at a position
    ApplyAssist {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// ID of the assist to apply
        assist_id: String,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Rename a symbol at a specific position
    RenameSymbol {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// New name for the symbol
        new_name: String,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Analyze a symbol completely - type, definition, implementations, callers, reference count
    AnalyzeSymbol {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Get the structure of a file (types, functions, impls, traits) without reading it
    GetFileOutline {
        /// Path to the Rust source file
        file_path: String,
    },

    /// Check if code compiles and get diagnostics with suggested fixes
    GetDiagnostics {
        /// Path to the Rust source file
        file_path: String,
    },

    /// Expand a macro at a specific position to see what it generates
    ExpandMacro {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Search for types, functions, or traits by name across the workspace
    SearchSymbols {
        /// The search query (fuzzy matched against symbol names)
        query: String,
        /// Maximum number of results to return
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Get function parameter info at a call site
    GetSignatureHelp {
        /// Path to the Rust source file
        file_path: String,
        /// Line number (1-based)
        line: u32,
        /// Column number (1-based)
        column: u32,
        /// Optional symbol name to search for near the coordinates
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Structural Search and Replace (SSR) - semantic find-and-replace for Rust code
    Ssr {
        /// The SSR pattern. Format: `search_pattern ==>> replacement_pattern`
        pattern: String,
        /// Optional file path for name resolution context
        #[arg(long)]
        context_file: Option<String>,
        /// Only show matches without applying changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Search for code patterns using SSR syntax (without replacement)
    SsrSearch {
        /// The search pattern (use $name for placeholders)
        pattern: String,
        /// Optional file path for name resolution context
        #[arg(long)]
        context_file: Option<String>,
    },
}

// For REPL use - reuses existing analyzer connection
pub async fn execute_analyzer_command_with_instance(
    command: AnalyzerCommand,
    analyzer: &mut RustAnalyzerish,
) -> Result<()> {
    match command {
        AnalyzerCommand::TypeHint {
            file_path,
            line,
            column,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.get_type_hint(&cursor).await {
                Ok(Some(type_info)) => {
                    println!("Type Hint:\n-----\n{}\n------", type_info);
                }
                Ok(None) => {
                    println!(
                        "No type information available at {}:{}:{}",
                        file_path, line, column
                    );
                }
                Err(e) => {
                    println!("Error getting type hint: {}", e);
                }
            }
        }
        AnalyzerCommand::GetDefinition {
            file_path,
            line,
            column,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.get_definition(&cursor).await {
                Ok(Some(definitions)) => {
                    println!("Found {} definition(s):", definitions.len());
                    for def in definitions {
                        println!("  {}", def);
                    }
                }
                Ok(None) => {
                    println!("No definitions found at {}:{}:{}", file_path, line, column);
                }
                Err(e) => {
                    println!("Error getting definitions: {}", e);
                }
            }
        }
        AnalyzerCommand::GetCompletions {
            file_path,
            line,
            column,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.get_completions(&cursor).await {
                Ok(Some(completions)) => {
                    println!(
                        "Available completions at {}:{}:{} ({} items):",
                        file_path,
                        line,
                        column,
                        completions.len()
                    );
                    for completion in completions {
                        println!("  {}", completion);
                    }
                }
                Ok(None) => {
                    println!("No completions found at {}:{}:{}", file_path, line, column);
                }
                Err(e) => {
                    println!("Error getting completions: {}", e);
                }
            }
        }
        AnalyzerCommand::FindReferences {
            file_path,
            line,
            column,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.find_references(&cursor).await {
                Ok(Some(references)) => {
                    println!("Found {} reference(s):", references.len());
                    for reference in references {
                        println!("  {}", reference);
                    }
                }
                Ok(None) => {
                    println!("No references found at {}:{}:{}", file_path, line, column);
                }
                Err(e) => {
                    println!("Error finding references: {}", e);
                }
            }
        }
        AnalyzerCommand::ViewInlayHints {
            file_path,
            start_line,
            end_line,
        } => {
            match analyzer
                .view_inlay_hints(&file_path, start_line, end_line)
                .await
            {
                Ok(annotated_content) => {
                    println!("File with inlay hints:");
                    println!("=====================================");
                    println!("{}", annotated_content);
                    println!("=====================================");
                }
                Err(e) => {
                    println!("Error viewing inlay hints: {}", e);
                }
            }
        }
        AnalyzerCommand::GetAssists {
            file_path,
            line,
            column,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.get_assists(&cursor).await {
                Ok(Some(assists)) => {
                    println!(
                        "Available assists at {}:{}:{} ({} items):",
                        file_path,
                        line,
                        column,
                        assists.len()
                    );
                    for assist in assists {
                        println!("  {} ({}): {}", assist.label, assist.id, assist.target);
                    }
                }
                Ok(None) => {
                    println!("No assists available at {}:{}:{}", file_path, line, column);
                }
                Err(e) => {
                    println!("Error getting assists: {}", e);
                }
            }
        }
        AnalyzerCommand::ApplyAssist {
            file_path,
            line,
            column,
            assist_id,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.apply_assist(&cursor, &assist_id).await {
                Ok(Some(source_change)) => {
                    println!("Successfully applied assist '{}':", assist_id);
                    for file_change in &source_change.file_changes {
                        println!("  Modified file: {}", file_change.file_path);
                        println!("    {} edits applied", file_change.edits.len());
                    }
                }
                Ok(None) => {
                    println!(
                        "Assist '{}' not available at {}:{}:{}",
                        assist_id, file_path, line, column
                    );
                }
                Err(e) => {
                    println!("Error applying assist '{}': {}", assist_id, e);
                }
            }
        }
        AnalyzerCommand::RenameSymbol {
            file_path,
            line,
            column,
            new_name,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.rename_symbol(&cursor, &new_name).await {
                Ok(Some(changes)) => {
                    println!(
                        "Rename successful! {} file(s) changed:",
                        changes.file_changes.len()
                    );
                    for change in &changes.file_changes {
                        println!("  {}: {} edit(s)", change.file_path, change.edits.len());
                    }
                }
                Ok(None) => {
                    println!(
                        "No symbol found to rename at {}:{}:{}",
                        file_path, line, column
                    );
                }
                Err(e) => {
                    println!("Error renaming symbol: {}", e);
                }
            }
        }
        AnalyzerCommand::AnalyzeSymbol {
            file_path,
            line,
            column,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.analyze_symbol(&cursor).await {
                Ok(analysis) => {
                    println!("{}", analysis);
                }
                Err(e) => {
                    println!("Error analyzing symbol: {}", e);
                }
            }
        }
        AnalyzerCommand::GetFileOutline { file_path } => {
            match analyzer.get_file_outline(&file_path).await {
                Ok(items) => {
                    if items.is_empty() {
                        println!("No structure items found in file.");
                    } else {
                        for item in items {
                            println!("{}", item);
                        }
                    }
                }
                Err(e) => {
                    println!("Error getting file outline: {}", e);
                }
            }
        }
        AnalyzerCommand::GetDiagnostics { file_path } => {
            match analyzer.get_diagnostics(&file_path).await {
                Ok(diagnostics) => {
                    if diagnostics.is_empty() {
                        println!("No diagnostics — code looks clean.");
                    } else {
                        for diag in diagnostics {
                            println!("{}\n", diag);
                        }
                    }
                }
                Err(e) => {
                    println!("Error getting diagnostics: {}", e);
                }
            }
        }
        AnalyzerCommand::ExpandMacro {
            file_path,
            line,
            column,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.expand_macro(&cursor).await {
                Ok(Some(expansion)) => {
                    println!("{}", expansion);
                }
                Ok(None) => {
                    println!("No macro found at this position to expand.");
                }
                Err(e) => {
                    println!("Error expanding macro: {}", e);
                }
            }
        }
        AnalyzerCommand::SearchSymbols { query, limit } => {
            match analyzer.search_symbols(&query, limit).await {
                Ok(results) => {
                    if results.is_empty() {
                        println!("No symbols found matching '{}'", query);
                    } else {
                        println!("Found {} symbol(s):", results.len());
                        for result in results {
                            println!("{}", result);
                        }
                    }
                }
                Err(e) => {
                    println!("Error searching symbols: {}", e);
                }
            }
        }
        AnalyzerCommand::GetSignatureHelp {
            file_path,
            line,
            column,
            symbol,
        } => {
            let cursor = CursorCoordinates {
                file_path: file_path.clone(),
                line,
                column,
                symbol,
            };

            match analyzer.get_signature_help(&cursor).await {
                Ok(Some(sig_info)) => {
                    println!("{}", sig_info);
                }
                Ok(None) => {
                    println!("No signature help available at this position.");
                }
                Err(e) => {
                    println!("Error getting signature help: {}", e);
                }
            }
        }
        AnalyzerCommand::Ssr {
            pattern,
            context_file,
            dry_run,
        } => {
            match analyzer
                .ssr(&pattern, context_file.as_deref(), dry_run)
                .await
            {
                Ok(result) => {
                    println!("{}", result);
                }
                Err(e) => {
                    println!("SSR error: {}", e);
                }
            }
        }
        AnalyzerCommand::SsrSearch {
            pattern,
            context_file,
        } => match analyzer.ssr_search(&pattern, context_file.as_deref()).await {
            Ok(matches) => {
                if matches.is_empty() {
                    println!("No matches found for pattern: {}", pattern);
                } else {
                    println!("Found {} match(es):\n", matches.len());
                    for m in matches {
                        println!("{}", m);
                    }
                }
            }
            Err(e) => {
                println!("SSR search error: {}", e);
            }
        },
    }
    Ok(())
}

// For CLI use - creates new analyzer instance for single command
pub(crate) async fn execute_analyzer_command(
    command: AnalyzerCommand,
    workspace_path: &str,
) -> Result<()> {
    let mut analyzer = RustAnalyzerishBuilder::from_file(workspace_path)?.build()?;
    execute_analyzer_command_with_instance(command, &mut analyzer).await
}

pub(crate) fn extract_workspace_path(command: &AnalyzerCommand) -> String {
    match command {
        AnalyzerCommand::TypeHint { file_path, .. }
        | AnalyzerCommand::GetDefinition { file_path, .. }
        | AnalyzerCommand::GetCompletions { file_path, .. }
        | AnalyzerCommand::FindReferences { file_path, .. }
        | AnalyzerCommand::ViewInlayHints { file_path, .. }
        | AnalyzerCommand::GetAssists { file_path, .. }
        | AnalyzerCommand::ApplyAssist { file_path, .. }
        | AnalyzerCommand::RenameSymbol { file_path, .. }
        | AnalyzerCommand::AnalyzeSymbol { file_path, .. }
        | AnalyzerCommand::GetFileOutline { file_path, .. }
        | AnalyzerCommand::GetDiagnostics { file_path, .. }
        | AnalyzerCommand::ExpandMacro { file_path, .. }
        | AnalyzerCommand::GetSignatureHelp { file_path, .. } => file_path.clone(),
        AnalyzerCommand::SearchSymbols { .. } => std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string()),
        AnalyzerCommand::Ssr { context_file, .. }
        | AnalyzerCommand::SsrSearch { context_file, .. } => {
            context_file.clone().unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            })
        }
    }
}
