//! Snapshot test file parsing
//!
//! Extracts snapshot content and ops from Rust test files.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use regex::Regex;

/// Extracted test info: snapshot content and ops
#[derive(Debug, Clone)]
pub struct TestInfo {
    pub snapshot: String,
    pub ops: Option<String>,
    pub options_setup: Option<String>,
}

/// Find all snapshot test files in a directory
pub fn find_snapshot_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().map(|e| e == "rs").unwrap_or(false) {
            let name = path.file_stem().unwrap().to_str().unwrap();
            // Skip mod.rs
            if name != "mod" {
                files.push(path);
            }
        }
    }
    
    files.sort();
    Ok(files)
}

/// Extract module name from file name
/// "000_ltr_spawning_single" -> "000_spawning_single"
/// "010_ltr_spawning_multiple" -> "010_spawning_multiple"
pub fn extract_module_name(file_name: &str) -> Option<String> {
    // Pattern: NNN_ltr_rest or NNN_rtl_rest
    let re = Regex::new(r"^(\d+)_(ltr|rtl)_(.+)$").unwrap();
    
    if let Some(caps) = re.captures(file_name) {
        let num = &caps[1];
        let rest = &caps[3];
        Some(format!("{}_{}", num, rest))
    } else {
        None
    }
}

/// Extract snapshots and ops from a Rust test file
/// Returns a map of snapshot_name -> TestInfo (snapshot content + ops)
/// 
/// For functions with multiple snapshots, they are named:
/// - Single snapshot: `fn_name`
/// - Multiple snapshots: `fn_name_1`, `fn_name_2`, etc.
pub fn extract_snapshots(content: &str) -> Result<BTreeMap<String, TestInfo>> {
    let mut snapshots = BTreeMap::new();
    
    // Regex to find test functions - handles #[test] on separate line from fn
    let fn_re = Regex::new(r#"fn\s+(\w+)\s*\(\)"#).unwrap();
    
    // Find all test functions
    let mut saw_test_attr = false;
    let mut current_fn: Option<String> = None;
    let mut brace_depth = 0;
    let mut in_function = false;
    let mut function_content = String::new();
    
    for line in content.lines() {
        // Check for #[test] attribute
        if line.trim() == "#[test]" {
            saw_test_attr = true;
            continue;
        }
        
        // Check for function start after #[test]
        if saw_test_attr {
            if let Some(caps) = fn_re.captures(line) {
                current_fn = Some(caps[1].to_string());
                in_function = false;
                function_content.clear();
                brace_depth = 0;
            }
            saw_test_attr = false;
        }
        
        if current_fn.is_some() {
            function_content.push_str(line);
            function_content.push('\n');
            
            // Track braces
            for c in line.chars() {
                match c {
                    '{' => {
                        brace_depth += 1;
                        in_function = true;
                    }
                    '}' => {
                        brace_depth -= 1;
                        if in_function && brace_depth == 0 {
                            // End of function - extract all snapshots and ops
                            if let Some(fn_name) = current_fn.take() {
                                let fn_tests = extract_all_tests_from_function(&function_content);
                                if fn_tests.len() == 1 {
                                    // Single snapshot - use function name directly
                                    snapshots.insert(fn_name, fn_tests.into_iter().next().unwrap());
                                } else if !fn_tests.is_empty() {
                                    // Multiple snapshots - number them
                                    for (i, test_info) in fn_tests.into_iter().enumerate() {
                                        let name = format!("{}_{}", fn_name, i + 1);
                                        snapshots.insert(name, test_info);
                                    }
                                }
                            }
                            in_function = false;
                            function_content.clear();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    
    Ok(snapshots)
}

/// Extract ALL tests (snapshots + ops) from a function body
/// Returns a Vec of TestInfo in order
fn extract_all_tests_from_function(content: &str) -> Vec<TestInfo> {
    let mut tests = Vec::new();
    let mut remaining = content;
    
    // Extract ops that appear before each snapshot
    while let Some((snapshot, ops, rest)) = extract_next_test(remaining) {
        tests.push(TestInfo {
            snapshot,
            ops,
            options_setup: None, // TODO: extract options setup if needed
        });
        remaining = rest;
    }
    
    tests
}

/// Extract the next test (snapshot + preceding ops) from content
fn extract_next_test(content: &str) -> Option<(String, Option<String>, &str)> {
    // First find the snapshot
    let (snapshot, remaining) = extract_next_snapshot(content)?;
    
    // Now look backwards from the snapshot to find the ops
    // Look for the last `let ops = [...]` or `let ops = vec![...]` before the snapshot
    let ops = extract_ops_before_snapshot(content);
    
    Some((snapshot, ops, remaining))
}

/// Extract ops array from content (looks for the last `let ops = [...]` pattern)
fn extract_ops_before_snapshot(content: &str) -> Option<String> {
    // Find all occurrences of `let ops = [`
    let mut last_ops_start = None;
    let mut search_start = 0;
    
    while let Some(idx) = content[search_start..].find("let ops = [") {
        let abs_idx = search_start + idx;
        last_ops_start = Some(abs_idx);
        search_start = abs_idx + 1;
    }
    
    let ops_start = last_ops_start?;
    
    // Find the matching closing bracket
    let after_bracket = ops_start + "let ops = [".len();
    let rest = &content[after_bracket..];
    
    let mut bracket_depth = 1;
    let mut end_idx = 0;
    
    for (i, c) in rest.char_indices() {
        match c {
            '[' => bracket_depth += 1,
            ']' => {
                bracket_depth -= 1;
                if bracket_depth == 0 {
                    end_idx = i;
                    break;
                }
            }
            _ => {}
        }
    }
    
    if bracket_depth != 0 {
        return None;
    }
    
    // Extract the ops content (without the outer brackets)
    let ops_content = &rest[..end_idx];
    
    // Clean up and format
    let ops_lines: Vec<&str> = ops_content.lines().collect();
    
    // Find minimum indentation
    let min_indent = ops_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    
    // Remove common indentation and rebuild
    let cleaned: Vec<String> = ops_lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                l[min_indent..].to_string()
            } else {
                l.trim().to_string()
            }
        })
        .collect();
    
    let ops_str = cleaned.join("\n").trim().to_string();
    
    if ops_str.is_empty() {
        None
    } else {
        Some(ops_str)
    }
}

/// Extract the next snapshot from content, returning (snapshot, remaining_content)
fn extract_next_snapshot(content: &str) -> Option<(String, &str)> {
    // Look for assert_snapshot! with @r" or @r#"
    // The snapshot is a multi-line raw string
    
    // Find the start of the snapshot
    let start_patterns = [("@r\"", "\""), ("@r#\"", "\"#")];
    let mut best_match: Option<(usize, usize, &str)> = None; // (start_idx, content_start, end_pattern)
    
    for (start_pat, end_pat) in &start_patterns {
        if let Some(idx) = content.find(start_pat) {
            let content_start = idx + start_pat.len();
            if best_match.is_none() || idx < best_match.unwrap().0 {
                best_match = Some((idx, content_start, end_pat));
            }
        }
    }
    
    let (_, content_start, end_pattern) = best_match?;
    
    // Find the end of the snapshot
    let rest = &content[content_start..];
    let end = rest.find(end_pattern)?;
    
    let snapshot_raw = &rest[..end];
    let remaining = &rest[end + end_pattern.len()..];
    
    // Clean up the snapshot - remove leading newline and trailing whitespace per line
    let lines: Vec<&str> = snapshot_raw.lines().collect();
    
    // Skip empty first line if present
    let lines: Vec<&str> = if lines.first().map(|l| l.is_empty()).unwrap_or(false) {
        lines[1..].to_vec()
    } else {
        lines
    };
    
    // Find minimum indentation (excluding empty lines)
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    
    // Remove common indentation
    let cleaned: Vec<String> = lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                l[min_indent..].to_string()
            } else {
                l.to_string()
            }
        })
        .collect();
    
    Some((cleaned.join("\n"), remaining))
}
