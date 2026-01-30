# Example Files for Testing rustbelt

This directory contains sample Rust files for testing the rust-analyzer MCP server functionality.

## Files

- `sample.rs` - A comprehensive example with various Rust constructs for testing type hints

## Testing Type Hints

You can test type hints at various positions in `sample.rs`:

### Basic Types
- Line 26, Col 16: `people` variable (HashMap<String, Person>)
- Line 28, Col 16: `person` variable (Person)
- Line 32, Col 16: `result` variable (f64)

### Complex Types
- Line 35, Col 16: `numbers` variable (Vec<i32>)
- Line 36, Col 16: `doubled` variable (Vec<i32>)
- Line 37, Col 16: `sum` variable (i32)
- Line 40, Col 16: `nested` variable (Vec<Option<Result<String, &str>>>)

### Function Parameters
- Line 53, Col 32: `people` parameter (&HashMap<String, Person>)
- Line 57, Col 16: `total_age` variable (u32)

### Generic Functions
- Line 66, Col 12: `fetch_data` function return type
- Line 72, Col 8: Generic function `process_items`

## Usage with MCP Client

```bash
# Start the mcp in stdio mode
cargo run --bin rustbelt -- mcp

# Then call the get_type_hint tool with:
{
  "file_path": "/absolute/path/to/examples/sample.rs",
  "line": 26,
  "column": 16
}
```
