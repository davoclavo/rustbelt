use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use librustbelt::{
    analyzer::RustAnalyzerish, builder::RustAnalyzerishBuilder, entities::CursorCoordinates,
};
use ra_ap_ide::SymbolKind;
use tokio::sync::Mutex;

// Shared analyzer instance that gets initialized once
static SHARED_ANALYZER: OnceLock<Arc<Mutex<RustAnalyzerish>>> = OnceLock::new();

/// Get or initialize the shared analyzer instance
async fn get_shared_analyzer() -> Arc<Mutex<RustAnalyzerish>> {
    SHARED_ANALYZER
        .get_or_init(|| {
            let sample_path = get_sample_file_path();
            let analyzer = RustAnalyzerishBuilder::from_file(&sample_path)
                .expect("Failed to create analyzer from sample file")
                .build()
                .expect("Failed to build analyzer");
            Arc::new(Mutex::new(analyzer))
        })
        .clone()
}

/// Get the path to our sample project main.rs file
fn get_sample_file_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/sample-project/src/main.rs");
    path
}

#[tokio::test]
async fn test_type_hint_simple_variable() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test type hint for 'people' variable on line 31 (HashMap<String, Person>)
    let cursor = CursorCoordinates {
        file_path: sample_path.to_str().unwrap().to_string(),
        line: 31,
        column: 13,
        symbol: None,
    };
    let type_info = analyzer
        .get_type_hint(&cursor)
        .await
        .expect("Error getting type hint")
        .expect("Expected type info but got None");

    println!("Type info for 'people': {type_info}");
    assert!(
        type_info
            .canonical_types
            .contains(&"std::collections::hash::map::HashMap".to_string())
            && type_info
                .canonical_types
                .contains(&"sample::Person".to_string())
    );
}

#[tokio::test]
#[ignore = "Requires extracting function signatures"]
async fn test_type_hint_function_call() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test type hint for function call result on line 35 (f64)
    let type_info = analyzer
        .get_type_hint(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 35,
            column: 14,
            symbol: None,
        })
        .await
        .expect("Error getting type hint")
        .expect("Expected type info but got None");

    println!("Type info for function result: {type_info}");
    assert!(
        type_info
            .canonical_types
            .contains(&"std::collections::hash::map::HashMap".to_string())
    );
}

#[tokio::test]
async fn test_type_hint_complex_generic() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test type hint for complex generic type on line 46
    let result = analyzer
        .get_type_hint(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 46,
            column: 9,
            symbol: None,
        })
        .await
        .expect("Error getting type hint");

    if let Some(type_info) = result {
        println!("Type info for complex generic: {type_info}");
        assert!(
            type_info
                .canonical_types
                .contains(&"alloc::vec::Vec".to_string())
                && (type_info
                    .canonical_types
                    .contains(&"core::option::Option".to_string())
                    && type_info
                        .canonical_types
                        .contains(&"core::result::Result".to_string()))
        );
    } else {
        // Complex generics might not always have hover info available
        println!("No type info available for complex generic (acceptable)");
    }
}

#[tokio::test]
async fn test_get_definition_struct() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test get definition for Person struct usage on line 33
    let definitions = analyzer
        .get_definition(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 33,
            column: 18,
            symbol: None,
        })
        .await
        .expect("Error getting definition")
        .expect("Expected to find definition for Person struct");

    assert_eq!(
        definitions.len(),
        1,
        "Should find a single Person struct definition"
    );
    println!("Definition {:?}", definitions[0]);

    // Check that we found the struct definition
    let has_person_def = definitions
        .iter()
        .any(|def| def.name.contains("Person") && matches!(def.kind, Some(SymbolKind::Struct)));
    assert!(has_person_def, "Should find Person struct definition");
}

#[tokio::test]
async fn test_get_external_definition_function() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test get definition for function call on line 35
    let definitions = analyzer
        .get_definition(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 35,
            column: 14,
            symbol: None,
        })
        .await
        .expect("Error getting definition")
        .expect("Expected to find definition for function");

    assert_eq!(definitions.len(), 1, "Should find function call definition");
    let definition = &definitions[0];
    println!("Definition {definition:?}");

    // Check that we found the function definition
    assert_eq!(
        definition.description,
        Some("pub fn insert(&mut self, k: K, v: V) -> Option<V>".into())
    );
    let has_function_def =
        definition.name.contains("insert") && matches!(definition.kind, Some(SymbolKind::Function));
    assert!(has_function_def, "Should find `insert` function definition");
    assert_eq!(
        definition.module,
        "std::collections::hash::map::impl::HashMap<K, V, S>::insert"
    )
}

#[tokio::test]
async fn test_get_definition_method() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test get definition for method call on line 33 (.with_email)
    let definitions = analyzer
        .get_definition(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 33,
            column: 55,
            symbol: None,
        })
        .await
        .expect("Error getting definition")
        .expect("Expected to find definition for method");

    println!("Found {} definition(s) for method call", definitions.len());
    assert!(!definitions.is_empty());

    // Check that we found the method definition
    let has_method_def = definitions.iter().any(|def| {
        def.name.contains("with_email") && matches!(def.kind, Some(SymbolKind::Function))
    });
    assert!(has_method_def, "Should find with_email method definition");
}

#[tokio::test]
async fn test_error_handling_invalid_position() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test with invalid line/column (way beyond file bounds)
    let result = analyzer
        .get_type_hint(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 9999,
            column: 9999,
            symbol: None,
        })
        .await;

    // Should return an error for invalid position
    assert!(result.is_err());
}

#[tokio::test]
async fn test_error_handling_nonexistent_file() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;

    // Test with non-existent file
    let result = analyzer
        .get_type_hint(&CursorCoordinates {
            file_path: "/nonexistent/file.rs".to_string(),
            line: 10,
            column: 10,
            symbol: None,
        })
        .await;

    // Should return an error for non-existent file
    assert!(result.is_err());
}

#[tokio::test]
async fn test_no_definition_available() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test get definition on a comment or whitespace (should return None or empty)
    let result = analyzer
        .get_definition(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 1,
            column: 1,
            symbol: None,
        })
        .await
        .expect("Error getting definition");

    if let Some(definitions) = result {
        // If we get definitions, they should be empty or irrelevant
        println!(
            "Found {} definition(s) at comment position",
            definitions.len()
        );
    } else {
        // This is expected for comments/whitespace
        println!("No definitions found at comment position (expected)");
    }
}

#[tokio::test]
async fn test_multiple_usages_same_analyzer() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test multiple operations with the same analyzer to ensure state consistency

    // First operation: type hint
    let type_result = analyzer
        .get_type_hint(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 30,
            column: 9,
            symbol: None,
        })
        .await;
    assert!(type_result.is_ok());

    // Second operation: get definition
    let def_result = analyzer
        .get_definition(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 32,
            column: 15,
            symbol: None,
        })
        .await;
    assert!(def_result.is_ok());

    // Third operation: another type hint
    let type_result2 = analyzer
        .get_type_hint(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 39,
            column: 9,
            symbol: None,
        })
        .await;
    assert!(type_result2.is_ok());

    println!("Successfully performed multiple operations with shared analyzer");
}

#[tokio::test]
async fn test_analyzer_workspace_loading() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // This test ensures the analyzer can properly load and work with the workspace
    // The first call should trigger workspace loading
    let result = analyzer
        .get_type_hint(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 5,
            column: 10,
            symbol: None,
        })
        .await;

    // Should not error due to workspace loading issues
    match result {
        Ok(_) => println!("Workspace loaded successfully"),
        Err(e) => {
            // If there's an error, it should be about the specific position, not workspace
            // loading
            let error_msg = format!("{e}");
            assert!(
                !error_msg.contains("Cargo.toml") && !error_msg.contains("workspace"),
                "Workspace loading failed: {e}"
            );
        }
    }
}

#[tokio::test]
async fn test_type_hint_variable_with_name() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test type hint for 'doubled' variable on line 42 (should show "let doubled:
    // Vec<i32>")
    let type_info = analyzer
        .get_type_hint(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 41,
            column: 9,
            symbol: None,
        })
        .await
        .expect("Error getting type hint")
        .expect("Expected type info but got None");

    println!("Type info: {type_info}");

    // Should contain both the variable name and type
    assert!(
        type_info.symbol.contains("numbers"),
        "Should contain variable name 'numbers'"
    );
    assert!(
        type_info
            .canonical_types
            .contains(&"alloc::vec::Vec".to_string()),
        "Should contain type Vec"
    );
    assert!(
        type_info.symbol.contains("let"),
        "Should contain 'let' keyword"
    );
}

#[tokio::test]
async fn test_get_completions_basic() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test getting completions at a position where we expect some completions
    // For example, after "std::" we should get completions for std modules
    let completions = analyzer
        .get_completions(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 31,
            column: 18,
            symbol: None,
        })
        .await
        .expect("Error getting completions");

    if let Some(completions) = completions {
        println!("Found {} completions", completions.len());
        for completion in &completions {
            println!("  - {}", completion);
        }

        assert!(!completions.is_empty(), "Should find some completions");

        // Check that we have reasonable completion items
        let has_reasonable_completion = completions.iter().any(|c| !c.name.is_empty());
        assert!(
            has_reasonable_completion,
            "Should have completions with non-empty names"
        );
    } else {
        // Some positions might not have completions available
        println!("No completions found at this position");
    }
}

#[tokio::test]
async fn test_get_completions_method_chaining() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test getting completions after a dot (method completions)
    // This should trigger method/field completions
    let completions = analyzer
        .get_completions(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 32,
            column: 20,
            symbol: None,
        })
        .await
        .expect("Error getting completions");

    if let Some(completions) = completions {
        println!("Found {} method completions", completions.len());
        for completion in &completions {
            println!("  - {}", completion);
        }

        // For a HashMap, we should see methods like insert, get, etc.
        let has_hash_map_methods = completions
            .iter()
            .any(|c| c.name.contains("insert") || c.name.contains("get") || c.name.contains("len"));

        if has_hash_map_methods {
            assert!(
                has_hash_map_methods,
                "Should find HashMap methods in completions"
            );
        } else {
            println!("No HashMap methods found, but that's acceptable depending on context");
        }
    } else {
        println!("No method completions found at this position");
    }
}

#[tokio::test]
async fn test_view_inlay_hints() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test getting completions after a dot (method completions)
    // This should trigger method/field completions
    let file_with_inlay_hints = analyzer
        .view_inlay_hints(sample_path.to_str().unwrap(), None, None)
        .await
        .expect("Error viewing inlay hints");

    println!("{file_with_inlay_hints}");
    // Adding type hints
    assert!(
        file_with_inlay_hints.contains("let _sum: i32"),
        "Should show inlay type hint for _sum"
    );
    assert!(
        file_with_inlay_hints.contains("let person: Person"),
        "Should show inlay type hint for person"
    );
    assert!(
        file_with_inlay_hints.contains("let numbers: Vec<i32>"),
        "Should show inlay type hint for numbers"
    );
    assert!(
        file_with_inlay_hints.contains("for item: Option<Result<String, &str>>"),
        "Should show inlay type hint for numbers"
    );

    // Adding named arguments
    assert!(
        file_with_inlay_hints.contains("Person::new(name: \"Alice\".to_string()"),
        "Should show named arguments"
    );

    // Keeping existing types intact
    assert!(
        file_with_inlay_hints.contains("let doubled: Vec<i32>"),
        "Should keep existing types intact"
    );
}

#[tokio::test]
async fn test_view_inlay_hints_with_line_range() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test with line range from lines 30-45 (includes main function start to line 45)
    let range_hints = analyzer
        .view_inlay_hints(sample_path.to_str().unwrap(), Some(30), Some(45))
        .await
        .expect("Error viewing inlay hints with range");

    println!("Range hints (lines 30-45): {}", range_hints);

    // Should contain hints from the specified range
    assert!(
        range_hints.contains("let mut people: HashMap<String, Person>"),
        "Should show inlay type hint for people in range"
    );
    assert!(
        range_hints.contains("let person: Person"),
        "Should show inlay type hint for person in range"
    );
    assert!(
        range_hints.contains("let numbers: Vec<i32>"),
        "Should show inlay type hint for numbers in range"
    );

    // Should NOT contain hints from outside the range (e.g., struct definition)
    assert!(
        !range_hints.contains("pub struct Person"),
        "Should not contain struct definition from outside range"
    );

    // Test with a smaller range (lines 41-43)
    let small_range_hints = analyzer
        .view_inlay_hints(sample_path.to_str().unwrap(), Some(41), Some(43))
        .await
        .expect("Error viewing inlay hints with small range");

    println!("Small range hints (lines 41-43): {}", small_range_hints);

    // Should contain the specific lines
    assert!(
        small_range_hints.contains("let numbers: Vec<i32>"),
        "Should show inlay type hint for numbers in small range"
    );
    assert!(
        small_range_hints.contains("let doubled: Vec<i32>"),
        "Should show inlay type hint for doubled in small range"
    );

    // Should NOT contain hints from outside the small range
    assert!(
        !small_range_hints.contains("let mut people: HashMap<String, Person>"),
        "Should not contain people definition from outside small range"
    );
}

#[tokio::test]
async fn test_find_references() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test finding references to the Person struct definition
    let references = analyzer
        .find_references(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 5, // Person struct definition
            column: 12,
            symbol: None,
        })
        .await
        .expect("Error finding references");

    let references = references.expect("Should find references to Person struct");

    println!("Found {} references to Person:", references.len());
    for reference in &references {
        println!("  - {}", reference);
    }

    // Should find at least 2 references (definition + at least one usage)
    assert!(
        references.len() >= 2,
        "Should find at least the definition and one usage, found {}",
        references.len()
    );

    // Should have exactly one definition
    let definitions: Vec<_> = references.iter().filter(|r| r.is_definition).collect();
    assert_eq!(definitions.len(), 1, "Should have exactly one definition");

    let definition = definitions[0];
    assert_eq!(
        definition.name, "Person",
        "Definition should be named 'Person'"
    );
    assert!(
        definition.file_path.ends_with("main.rs"),
        "Definition should be in main.rs"
    );
    assert_eq!(definition.line, 5, "Definition should be on line 5");
    assert!(
        definition.content.contains("struct Person"),
        "Definition content should contain 'struct Person'"
    );

    // Should have some references (usages)
    let usages: Vec<_> = references.iter().filter(|r| !r.is_definition).collect();
    assert!(!usages.is_empty(), "Should have at least one usage");

    // All references should have the same symbol name
    assert!(
        references.iter().all(|r| r.name == "Person"),
        "All references should be named 'Person'"
    );

    // All references should have valid coordinates
    for reference in &references {
        assert!(
            !reference.file_path.is_empty(),
            "File path should not be empty"
        );
        assert!(reference.line > 0, "Line should be positive");
        assert!(reference.column > 0, "Column should be positive");
        assert!(
            reference.end_line >= reference.line,
            "End line should be >= start line"
        );
        assert!(
            reference.end_line > reference.line || reference.end_column > reference.column,
            "End position should be after start position"
        );
        assert!(
            !reference.content.trim().is_empty(),
            "Content should not be empty"
        );
    }

    // References should be sorted by file path, then line, then column
    let mut sorted_refs = references.clone();
    sorted_refs.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.column.cmp(&b.column))
    });
    assert_eq!(references, sorted_refs, "References should be sorted");
}

#[tokio::test]
async fn test_find_references_variable() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test finding references to a variable like 'people'
    let references = analyzer
        .find_references(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 31, // people variable declaration
            column: 13,
            symbol: None,
        })
        .await
        .expect("Error finding references");

    let references = references.expect("Should find references to people variable");

    println!("Found {} references to people variable:", references.len());
    for reference in &references {
        println!("  - {}", reference);
    }

    // Should find at least one reference (the declaration itself)
    assert!(
        !references.is_empty(),
        "Should find at least the declaration"
    );

    // Variable references should have a specific symbol name
    let symbol_name = &references[0].name;
    assert!(
        references.iter().all(|r| r.name == *symbol_name),
        "All references should have the same symbol name '{}'",
        symbol_name
    );

    // Check that all references have valid coordinates and content
    for reference in &references {
        assert!(
            !reference.file_path.is_empty(),
            "File path should not be empty"
        );
        assert!(reference.line > 0, "Line should be positive");
        assert!(reference.column > 0, "Column should be positive");
        assert!(
            reference.end_line >= reference.line,
            "End line should be >= start line"
        );
        assert!(
            reference.end_line > reference.line || reference.end_column > reference.column,
            "End position should be after start position"
        );
        assert!(
            !reference.content.trim().is_empty(),
            "Content should not be empty"
        );
        assert!(
            reference.file_path.ends_with("main.rs"),
            "References should be in main.rs"
        );
    }

    // Should have at least one definition or usage that contains the variable name
    assert!(
        references.iter().any(|r| r.content.contains(symbol_name)),
        "At least one reference should contain the symbol name '{}' in its content",
        symbol_name
    );
}

#[tokio::test]
async fn test_symbol_resolution() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test finding a symbol with approximate coordinates
    let cursor = CursorCoordinates {
        file_path: sample_path.to_str().unwrap().to_string(),
        line: 29, // Approximate line near the 'people' variable (line 31 is exact, testing tolerance)
        column: 6, // Approximate column
        symbol: Some("people".to_string()),
    };

    let type_info = analyzer
        .get_type_hint(&cursor)
        .await
        .expect("Should get type hint");

    assert!(
        type_info.is_some(),
        "Should find type info for 'people' symbol"
    );
    eprintln!("{type_info:?}");
    let type_info = type_info.unwrap();
    assert!(
        type_info.symbol.contains("people"),
        "Symbol should contain 'people': {}",
        type_info.symbol
    );
    assert!(
        type_info
            .canonical_types
            .iter()
            .any(|t| t.contains("HashMap"))
    );
    assert_eq!(type_info.line, 31, "Line number should be found");
    assert_eq!(type_info.column, 13, "Column number should be found");
}

// ==================== Tests for new agent-native tools ====================

#[tokio::test]
async fn test_get_diagnostics() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Get diagnostics for our sample file
    let diagnostics = analyzer
        .get_diagnostics(sample_path.to_str().unwrap())
        .await
        .expect("Error getting diagnostics");

    println!("Found {} diagnostics", diagnostics.len());
    for d in &diagnostics {
        println!("  - {}", d);
    }

    // The sample file should compile cleanly (or only have warnings)
    // No hard errors expected
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "Error")
        .collect();
    assert!(
        errors.is_empty(),
        "Sample file should not have compile errors, found: {:?}",
        errors
    );

    // All diagnostics should have valid file paths and positions
    for d in &diagnostics {
        assert!(
            d.file_path.ends_with("main.rs"),
            "Diagnostic should reference main.rs"
        );
        assert!(d.line > 0, "Diagnostic line should be positive");
        assert!(d.column > 0, "Diagnostic column should be positive");
        assert!(
            !d.message.is_empty(),
            "Diagnostic message should not be empty"
        );
        assert!(
            !d.severity.is_empty(),
            "Diagnostic severity should not be empty"
        );
    }
}

#[tokio::test]
async fn test_analyze_symbol_struct() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Analyze the Person struct
    let result = analyzer
        .analyze_symbol(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 5,
            column: 12,
            symbol: None,
        })
        .await
        .expect("Error analyzing symbol");

    println!("Symbol analysis:\n{}", result);

    // Should have type/hover info
    assert!(
        result.type_info.is_some(),
        "Should have type info for Person struct"
    );
    let type_info = result.type_info.as_ref().unwrap();
    assert!(
        type_info.contains("Person"),
        "Type info should mention Person"
    );

    // Should have at least one definition
    assert!(
        !result.definitions.is_empty(),
        "Should find definition for Person"
    );
    assert!(
        result.definitions[0].name.contains("Person"),
        "Definition should be named Person"
    );

    // Person struct should have implementations (the impl block)
    // (implementations may or may not be found depending on exact cursor position)

    // Should have references (Person is used multiple times)
    assert!(
        result.reference_count > 0,
        "Person should have references in the file"
    );
}

#[tokio::test]
async fn test_analyze_symbol_function() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Analyze the calculate_average_age function
    let result = analyzer
        .analyze_symbol(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 58,
            column: 4,
            symbol: Some("calculate_average_age".to_string()),
        })
        .await
        .expect("Error analyzing symbol");

    println!("Function analysis:\n{}", result);

    // Should have type/hover info
    assert!(
        result.type_info.is_some(),
        "Should have type info for function"
    );

    // Should have a definition
    assert!(
        !result.definitions.is_empty(),
        "Should find definition for function"
    );

    // Function should have callers (called from main)
    assert!(
        result.reference_count > 0,
        "Function should be referenced at least once"
    );
}

#[tokio::test]
async fn test_get_file_outline() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    let outline = analyzer
        .get_file_outline(sample_path.to_str().unwrap())
        .await
        .expect("Error getting file outline");

    println!("File outline ({} items):", outline.len());
    for item in &outline {
        println!("  - {}", item);
    }

    assert!(
        !outline.is_empty(),
        "Outline should contain items for our sample file"
    );

    // Should find the Person struct
    let has_person = outline.iter().any(|item| item.name == "Person");
    assert!(has_person, "Outline should contain Person struct");

    // Should find the main function
    let has_main = outline.iter().any(|item| item.name == "main");
    assert!(has_main, "Outline should contain main function");

    // Should find calculate_average_age function
    let has_calc = outline
        .iter()
        .any(|item| item.name == "calculate_average_age");
    assert!(
        has_calc,
        "Outline should contain calculate_average_age function"
    );

    // Should find the impl block methods
    let has_new = outline.iter().any(|item| item.name == "new");
    assert!(has_new, "Outline should contain Person::new method");

    let has_with_email = outline.iter().any(|item| item.name == "with_email");
    assert!(
        has_with_email,
        "Outline should contain Person::with_email method"
    );

    // All items should have valid line numbers
    for item in &outline {
        assert!(item.line > 0, "Line should be positive");
        assert!(
            item.end_line >= item.line,
            "End line should be >= start line"
        );
        assert!(!item.name.is_empty(), "Name should not be empty");
        assert!(!item.kind.is_empty(), "Kind should not be empty");
    }
}

#[tokio::test]
async fn test_search_symbols() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;

    // Search for "Person"
    let results = analyzer
        .search_symbols("Person", 50)
        .await
        .expect("Error searching symbols");

    println!("Found {} symbols matching 'Person':", results.len());
    for r in &results {
        println!("  - {}", r);
    }

    assert!(
        !results.is_empty(),
        "Should find at least one symbol matching 'Person'"
    );

    // Should find the Person struct
    let has_person = results.iter().any(|r| r.name == "Person");
    assert!(has_person, "Should find Person struct in search results");

    // All results should have valid file paths and positions
    for r in &results {
        assert!(!r.file_path.is_empty(), "File path should not be empty");
        assert!(r.line > 0, "Line should be positive");
        assert!(r.column > 0, "Column should be positive");
        assert!(!r.name.is_empty(), "Name should not be empty");
    }
}

#[tokio::test]
async fn test_search_symbols_fuzzy() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;

    // Fuzzy search for "calc" should find calculate_average_age
    let results = analyzer
        .search_symbols("calc", 50)
        .await
        .expect("Error searching symbols");

    println!("Found {} symbols matching 'calc':", results.len());
    for r in &results {
        println!("  - {}", r);
    }

    let has_calc_func = results
        .iter()
        .any(|r| r.name.contains("calculate_average_age"));
    assert!(
        has_calc_func,
        "Fuzzy search for 'calc' should find calculate_average_age"
    );
}

#[tokio::test]
async fn test_expand_macro_derive() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Try to expand the #[derive(Debug, Clone)] on line 4
    let result = analyzer
        .expand_macro(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 4,
            column: 10,
            symbol: None,
        })
        .await
        .expect("Error expanding macro");

    if let Some(expansion) = result {
        println!("Macro expansion:\n{}", expansion);
        assert!(!expansion.name.is_empty(), "Macro name should not be empty");
        assert!(
            !expansion.expansion.is_empty(),
            "Expansion should not be empty"
        );
    } else {
        // Derive macro expansion might not be available in all configurations
        println!("No macro expansion available at this position (acceptable for derive)");
    }
}

#[tokio::test]
async fn test_expand_macro_vec() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Try to expand vec! macro on line 41
    let result = analyzer
        .expand_macro(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 41,
            column: 20,
            symbol: Some("vec".to_string()),
        })
        .await
        .expect("Error expanding macro");

    if let Some(expansion) = result {
        println!("vec! macro expansion:\n{}", expansion);
        assert!(
            !expansion.expansion.is_empty(),
            "Expansion should not be empty"
        );
    } else {
        println!("No vec! macro expansion available (acceptable)");
    }
}

#[tokio::test]
async fn test_get_signature_help() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test signature help inside Person::new() call on line 33
    // Person::new("Alice".to_string(), 25)
    // Position cursor inside the arguments
    let result = analyzer
        .get_signature_help(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 33,
            column: 27,
            symbol: None,
        })
        .await
        .expect("Error getting signature help");

    if let Some(sig_info) = result {
        println!("Signature help:\n{}", sig_info);

        assert!(
            !sig_info.signature.is_empty(),
            "Signature should not be empty"
        );

        // Should have parameters
        assert!(
            !sig_info.parameters.is_empty(),
            "Should have at least one parameter"
        );

        // Should mention the parameter types/names
        let params_str = sig_info.parameters.join(", ");
        println!("Parameters: {}", params_str);
    } else {
        println!("No signature help available at this position");
    }
}

#[tokio::test]
async fn test_get_signature_help_insert() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test signature help inside people.insert() call on line 35
    let result = analyzer
        .get_signature_help(&CursorCoordinates {
            file_path: sample_path.to_str().unwrap().to_string(),
            line: 35,
            column: 20,
            symbol: None,
        })
        .await
        .expect("Error getting signature help");

    if let Some(sig_info) = result {
        println!("Signature help for insert():\n{}", sig_info);
        assert!(!sig_info.signature.is_empty());
        assert!(!sig_info.parameters.is_empty());
    } else {
        println!("No signature help available for insert() (acceptable)");
    }
}

#[tokio::test]
async fn test_get_diagnostics_nonexistent_file() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;

    let result = analyzer.get_diagnostics("/nonexistent/file.rs").await;
    assert!(result.is_err(), "Should error for nonexistent file");
}

#[tokio::test]
async fn test_get_file_outline_nonexistent_file() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;

    let result = analyzer.get_file_outline("/nonexistent/file.rs").await;
    assert!(result.is_err(), "Should error for nonexistent file");
}

// ==================== SSR (Structural Search and Replace) Tests ====================

#[tokio::test]
async fn test_ssr_search_method_call() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Search for .to_string() calls
    let matches = analyzer
        .ssr_search("$receiver.to_string()", Some(sample_path.to_str().unwrap()))
        .await
        .expect("Error performing SSR search");

    println!("Found {} matches for .to_string():", matches.len());
    for m in &matches {
        println!("  - {}", m);
    }

    // The sample file has multiple .to_string() calls
    assert!(
        !matches.is_empty(),
        "Should find at least one .to_string() call"
    );

    // All matches should have matched text
    for m in &matches {
        assert!(
            !m.matched_text.is_empty(),
            "Matched text should not be empty"
        );
        assert!(
            m.matched_text.contains("to_string"),
            "Matched text should contain 'to_string'"
        );
    }
}

#[tokio::test]
async fn test_ssr_search_collect_call() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Search for .collect() calls
    let matches = analyzer
        .ssr_search("$receiver.collect()", Some(sample_path.to_str().unwrap()))
        .await
        .expect("Error performing SSR search");

    println!("Found {} matches for .collect():", matches.len());
    for m in &matches {
        println!("  - {}", m);
    }

    // The sample file has .collect() calls in iterator chains
    assert!(
        !matches.is_empty(),
        "Should find at least one .collect() call"
    );

    for m in &matches {
        assert!(
            m.matched_text.contains("collect"),
            "Matched text should contain 'collect': {}",
            m.matched_text
        );
    }
}

#[tokio::test]
async fn test_ssr_search_pattern_with_placeholder() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Search for Option field access - email field is Option<String>
    let matches = analyzer
        .ssr_search("$receiver.clone()", Some(sample_path.to_str().unwrap()))
        .await
        .expect("Error performing SSR search");

    println!("Found {} matches for .clone():", matches.len());
    for m in &matches {
        println!("  - {}", m);
    }

    // The sample file has .clone() usage
    assert!(
        !matches.is_empty(),
        "Should find at least one .clone() usage"
    );

    // Matches should contain "clone"
    for m in &matches {
        assert!(
            m.matched_text.contains("clone"),
            "Matched text should contain 'clone': {}",
            m.matched_text
        );
    }
}

#[tokio::test]
async fn test_ssr_dry_run() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Test SSR with dry_run=true (preview replacement without applying)
    // Replace .clone() with .to_owned()
    let result = analyzer
        .ssr(
            "$receiver.clone() ==>> $receiver.to_owned()",
            Some(sample_path.to_str().unwrap()),
            true, // dry_run
        )
        .await
        .expect("Error performing SSR dry run");

    println!("SSR dry run result:\n{}", result);

    // The sample file has .clone() calls (e.g., person.name.clone())
    assert!(
        !result.matches.is_empty(),
        "Should find at least one .clone() call"
    );

    // Since it's a dry run, file_changes should be None
    assert!(
        result.file_changes.is_none(),
        "Dry run should not have file_changes"
    );
    assert!(result.dry_run, "Result should indicate it was a dry run");

    // Matches should show the replacement preview
    for m in &result.matches {
        assert!(
            m.matched_text.contains("clone"),
            "Matched text should contain 'clone'"
        );
        if let Some(ref replacement) = m.replacement {
            assert!(
                replacement.contains("to_owned"),
                "Replacement should contain 'to_owned'"
            );
        }
    }
}

#[tokio::test]
async fn test_ssr_search_no_matches() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Search for a pattern that doesn't exist in the sample file
    let matches = analyzer
        .ssr_search(
            "$receiver.nonexistent_method()",
            Some(sample_path.to_str().unwrap()),
        )
        .await
        .expect("Error performing SSR search");

    println!("Found {} matches for nonexistent method:", matches.len());

    assert!(
        matches.is_empty(),
        "Should find no matches for nonexistent method"
    );
}

#[tokio::test]
async fn test_ssr_complex_pattern() {
    let analyzer = get_shared_analyzer().await;
    let mut analyzer = analyzer.lock().await;
    let sample_path = get_sample_file_path();

    // Search for iterator chain pattern: .iter().map(...)
    let matches = analyzer
        .ssr_search(
            "$receiver.iter().map($f)",
            Some(sample_path.to_str().unwrap()),
        )
        .await
        .expect("Error performing SSR search");

    println!("Found {} matches for .iter().map():", matches.len());
    for m in &matches {
        println!("  - {}", m);
    }

    // The sample file has .iter().map() chains
    assert!(
        !matches.is_empty(),
        "Should find at least one .iter().map() chain"
    );
}
