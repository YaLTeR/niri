use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use miette::{Context as _, IntoDiagnostic as _};

#[derive(Debug, Clone)]
pub struct IncludeResult {
    pub content: String, // The fully resolved configuration content
    pub included_files: Vec<PathBuf>,
}

pub fn resolve_includes(main_path: &Path) -> miette::Result<IncludeResult> {
    let mut included_files = Vec::new();
    let mut inclusion_stack = Vec::new();
    let mut visited = HashSet::new();

    let content = resolve_includes_recursive(
        main_path,
        &mut included_files,
        &mut inclusion_stack,
        &mut visited,
    )?;

    Ok(IncludeResult {
        content,
        included_files,
    })
}

fn resolve_includes_recursive(
    path: &Path,
    included_files: &mut Vec<PathBuf>,
    inclusion_stack: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,
) -> miette::Result<String> {
    let canonical_path = path
        .canonicalize()
        .into_diagnostic()
        .with_context(|| format!("failed to canonicalize path {path:?}"))?;

    // Check for circular dependencies
    if inclusion_stack.contains(&canonical_path) {
        let cycle_start = inclusion_stack
            .iter()
            .position(|p| p == &canonical_path)
            .unwrap();
        let cycle: Vec<_> = inclusion_stack[cycle_start..]
            .iter()
            .chain(std::iter::once(&canonical_path))
            .map(|p| p.display().to_string())
            .collect();

        return Err(miette::miette!(
            "circular dependency detected: {}",
            cycle.join(" -> ")
        ));
    }

    if !visited.insert(canonical_path.clone()) {
        // Already processed this file, return empty content to avoid duplicates
        return Ok(String::new());
    }

    inclusion_stack.push(canonical_path.clone());
    included_files.push(canonical_path.clone());

    let content = fs::read_to_string(&canonical_path)
        .into_diagnostic()
        .with_context(|| format!("error reading {canonical_path:?}"))?;

    let mut result = String::new();
    let current_dir = canonical_path
        .parent()
        .ok_or_else(|| miette::miette!("config file has no parent directory"))?;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for include directive
        if trimmed.starts_with("include") {
            if let Some(include_path) = parse_include_directive(trimmed)? {
                let resolved_path = resolve_include_path(&include_path, current_dir)?;
                let included_content = resolve_includes_recursive(
                    &resolved_path,
                    included_files,
                    inclusion_stack,
                    visited,
                )?;

                if !included_content.is_empty() {
                    if !result.is_empty() && !result.ends_with('\n') {
                        result.push('\n');
                    }
                    result.push_str(&included_content);
                    if !included_content.ends_with('\n') {
                        result.push('\n');
                    }
                }
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    inclusion_stack.pop();
    Ok(result)
}

fn parse_include_directive(line: &str) -> miette::Result<Option<String>> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("include") {
        return Ok(None);
    }

    let mut rest = &trimmed["include".len()..];
    rest = rest.trim_start();

    if let Some(path) = parse_raw_string(rest) {
        if path.is_empty() {
            return Err(miette::miette!("include directive has empty path: {line}"));
        }
        return Ok(Some(path));
    }

    if let Some(path) = parse_normal_string(rest) {
        if path.is_empty() {
            return Err(miette::miette!("include directive has empty path: {line}"));
        }
        return Ok(Some(path));
    }

    Err(miette::miette!(
        "include directive must have quoted path: {line}"
    ))
}

fn parse_raw_string(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;

    let has_r = matches!(bytes.get(0), Some(b'r'));
    if has_r {
        i += 1;
    }

    let mut hashes = 0;
    while matches!(bytes.get(i), Some(b'#')) {
        hashes += 1;
        i += 1;
    }

    if hashes == 0 && !has_r {
        return None;
    }

    if !matches!(bytes.get(i), Some(b'"')) {
        return None;
    }
    i += 1;

    let start_content = i;

    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut j = i + 1;
            let mut k = 0;
            while k < hashes && matches!(bytes.get(j), Some(b'#')) {
                j += 1;
                k += 1;
            }
            if k == hashes {
                let content = &s[start_content..i];
                return Some(content.to_string());
            }
        }
        i += 1;
    }

    None
}

fn parse_normal_string(s: &str) -> Option<String> {
    let mut iter = s.chars();
    if iter.next()? != '"' {
        return None;
    }

    let mut out = String::new();
    let mut escaped = false;

    for ch in iter {
        if escaped {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
            escaped = false;
        } else {
            match ch {
                '\\' => escaped = true,
                '"' => return Some(out),
                c => out.push(c),
            }
        }
    }
    None
}

fn resolve_include_path(include_path: &str, current_dir: &Path) -> miette::Result<PathBuf> {
    let path = Path::new(include_path);

    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    let resolved = current_dir.join(path);

    if !resolved.exists() {
        return Err(miette::miette!(
            "included file not found: {resolved:?} (from {include_path:?})"
        ));
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs as unix_fs;

    use xshell::Shell;

    use super::*;

    #[test]
    fn test_parse_include_directive() {
        assert_eq!(
            parse_include_directive("include \"config.kdl\"").unwrap(),
            Some("config.kdl".to_string())
        );

        assert_eq!(
            parse_include_directive("  include \"path/to/file.kdl\"  ").unwrap(),
            Some("path/to/file.kdl".to_string())
        );

        assert_eq!(
            parse_include_directive("include r#\"config.kdl\"#").unwrap(),
            Some("config.kdl".to_string())
        );

        assert_eq!(
            parse_include_directive("  include r#\"path/to/file.kdl\"#  ").unwrap(),
            Some("path/to/file.kdl".to_string())
        );

        assert_eq!(parse_include_directive("layout { gaps 10 }").unwrap(), None);

        assert!(parse_include_directive("include").is_err());
        assert!(parse_include_directive("include \"\"").is_err());
        assert!(parse_include_directive("include r#\"\"#").is_err());
        assert!(parse_include_directive("include unquoted").is_err());
    }

    #[test]
    fn include_normal_unescapes_backslashes() {
        assert_eq!(
            parse_include_directive(r#"include "my\\dir""#).unwrap(),
            Some(r"my\dir".to_string())
        );
    }

    #[test]
    fn include_raw_preserves_backslashes() {
        assert_eq!(
            parse_include_directive(r##"include r#"my\\dir"#"##).unwrap(),
            Some(r"my\\dir".to_string())
        );
    }

    #[test]
    fn include_kdl_shorthand_raw_preserves_backslashes() {
        assert_eq!(
            parse_include_directive(r##"include #"my\\dir"#"##).unwrap(),
            Some(r"my\\dir".to_string())
        );
    }

    #[test]
    fn include_allows_trailing_comment() {
        assert_eq!(
            parse_include_directive(r#"include "a.kdl" // note"#).unwrap(),
            Some("a.kdl".to_string())
        );
    }

    #[test]
    fn include_handles_quotes_and_common_escapes() {
        assert_eq!(
            parse_include_directive(r#"include "quote:\"x\"""#).unwrap(),
            Some(r#"quote:"x""#.to_string())
        );
        assert_eq!(
            parse_include_directive(r#"include "line1\nline2""#).unwrap(),
            Some("line1\nline2".to_string())
        );
    }

    #[test]
    fn include_multi_hash_raw() {
        assert_eq!(
            parse_include_directive(r####"include r###"weird "# inside"###"####).unwrap(),
            Some("weird \"# inside".to_string())
        );
        assert_eq!(
            parse_include_directive(r####"include ###"/some/file\withbackslash"###"####).unwrap(),
            Some("/some/file\\withbackslash".to_string())
        );
    }

    #[test]
    fn include_errors_on_empty_or_unquoted() {
        assert!(parse_include_directive("include").is_err());
        assert!(parse_include_directive(r#"include """#).is_err());
        assert!(parse_include_directive(r##"include r#""#"##).is_err());
        assert!(parse_include_directive("include unquoted").is_err());
    }

    #[test]
    fn test_nested_includes_two_levels() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create level2.kdl
        let level2_content = r#"
layout {
    gaps 20
}
"#;
        fs::write(temp_path.join("level2.kdl"), level2_content).unwrap();

        // Create level1.kdl that includes level2.kdl
        let level1_content = r#"
include "level2.kdl"
input {
    keyboard {
        repeat-rate 30
    }
}
"#;
        fs::write(temp_path.join("level1.kdl"), level1_content).unwrap();

        // Create main.kdl that includes level1.kdl
        let main_content = r#"
include "level1.kdl"
output "eDP-1" {
    scale 2
}
"#;
        fs::write(temp_path.join("main.kdl"), main_content).unwrap();

        let result = resolve_includes(&temp_path.join("main.kdl")).unwrap();

        // Should include content from all three files
        assert!(result.content.contains("scale 2"));
        assert!(result.content.contains("repeat-rate 30"));
        assert!(result.content.contains("gaps 20"));

        // Should track all processed files (main + 2 includes)
        assert_eq!(result.included_files.len(), 3);
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "main.kdl"));
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "level1.kdl"));
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "level2.kdl"));
    }

    #[test]
    fn test_nested_includes_three_levels() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create level3.kdl (deepest level)
        let level3_content = r#"
animations {
    slowdown 2.0
}
"#;
        fs::write(temp_path.join("level3.kdl"), level3_content).unwrap();

        // Create level2.kdl that includes level3.kdl
        let level2_content = r#"
include "level3.kdl"
layout {
    gaps 15
}
"#;
        fs::write(temp_path.join("level2.kdl"), level2_content).unwrap();

        // Create level1.kdl that includes level2.kdl
        let level1_content = r#"
include "level2.kdl"
input {
    mouse {
        accel-speed 0.5
    }
}
"#;
        fs::write(temp_path.join("level1.kdl"), level1_content).unwrap();

        // Create main.kdl that includes level1.kdl
        let main_content = r#"
include "level1.kdl"
binds {
    Mod+Q { close-window; }
}
"#;
        fs::write(temp_path.join("main.kdl"), main_content).unwrap();

        let result = resolve_includes(&temp_path.join("main.kdl")).unwrap();

        // Should include content from all four files
        assert!(result.content.contains("close-window"));
        assert!(result.content.contains("accel-speed 0.5"));
        assert!(result.content.contains("gaps 15"));
        assert!(result.content.contains("slowdown 2.0"));

        // Should track all processed files (main + 3 includes)
        assert_eq!(result.included_files.len(), 4);
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "main.kdl"));
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "level1.kdl"));
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "level2.kdl"));
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "level3.kdl"));
    }

    #[test]
    fn test_include_cycle_detection() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create a.kdl that includes b.kdl
        let a_content = r#"
include "b.kdl"
layout { gaps 10 }
"#;
        fs::write(temp_path.join("a.kdl"), a_content).unwrap();

        // Create b.kdl that includes c.kdl
        let b_content = r#"
include "c.kdl"
input { keyboard { repeat-rate 25 } }
"#;
        fs::write(temp_path.join("b.kdl"), b_content).unwrap();

        // Create c.kdl that includes a.kdl (creating a cycle)
        let c_content = r#"
include "a.kdl"
binds { Mod+Q { quit; } }
"#;
        fs::write(temp_path.join("c.kdl"), c_content).unwrap();

        let result = resolve_includes(&temp_path.join("a.kdl"));

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("circular dependency"));
        assert!(
            err_msg.contains("a.kdl") || err_msg.contains("b.kdl") || err_msg.contains("c.kdl")
        );
    }

    #[test]
    fn test_direct_self_include_cycle() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create a file that includes itself
        let self_include_content = r#"
include "self_include.kdl"
layout { gaps 5 }
"#;
        fs::write(temp_path.join("self_include.kdl"), self_include_content).unwrap();

        let result = resolve_includes(&temp_path.join("self_include.kdl"));

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("circular dependency"));
    }

    #[test]
    fn test_relative_paths_from_nested_includes() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create subdirectory structure
        let subdir = temp_path.join("configs");
        fs::create_dir(&subdir).unwrap();
        let deep_subdir = subdir.join("deep");
        fs::create_dir(&deep_subdir).unwrap();

        // Create deep/shared.kdl
        let shared_content = r#"
animations {
    slowdown 1.5
}
"#;
        fs::write(deep_subdir.join("shared.kdl"), shared_content).unwrap();

        // Create configs/layout.kdl that includes ../configs/deep/shared.kdl using relative path
        let layout_content = r#"
include "deep/shared.kdl"
layout {
    gaps 8
}
"#;
        fs::write(subdir.join("layout.kdl"), layout_content).unwrap();

        // Create main.kdl that includes configs/layout.kdl
        let main_content = r#"
include "configs/layout.kdl"
input {
    keyboard {
        repeat-delay 500
    }
}
"#;
        fs::write(temp_path.join("main.kdl"), main_content).unwrap();

        let result = resolve_includes(&temp_path.join("main.kdl")).unwrap();

        // Should resolve all relative paths correctly
        assert!(result.content.contains("repeat-delay 500"));
        assert!(result.content.contains("gaps 8"));
        assert!(result.content.contains("slowdown 1.5"));

        assert_eq!(result.included_files.len(), 3);
    }

    #[test]
    fn test_symlinked_includes() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create the actual config file
        let actual_config_content = r#"
layout {
    gaps 12
}
"#;
        let actual_file = temp_path.join("actual_config.kdl");
        fs::write(&actual_file, actual_config_content).unwrap();

        // Create a symlink to the actual file
        let symlink_file = temp_path.join("symlink_config.kdl");
        unix_fs::symlink(&actual_file, &symlink_file).unwrap();

        // Create main config that includes the symlink
        let main_content = r#"
include "symlink_config.kdl"
input {
    touchpad {
        tap
    }
}
"#;
        fs::write(temp_path.join("main.kdl"), main_content).unwrap();

        let result = resolve_includes(&temp_path.join("main.kdl")).unwrap();

        // Should resolve symlink and include content
        assert!(result.content.contains("tap"));
        assert!(result.content.contains("gaps 12"));

        // Should track both main file and the canonical path (not the symlink path)
        assert_eq!(result.included_files.len(), 2);
        // Should include both main.kdl and actual_config.kdl (canonical path)
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "main.kdl"));
        assert!(result
            .included_files
            .iter()
            .any(|p| p.file_name().unwrap() == "actual_config.kdl"));
    }

    #[test]
    fn test_missing_included_file_error() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create a config that includes a non-existent file
        let main_content = r#"
include "nonexistent.kdl"
layout {
    gaps 10
}
"#;
        fs::write(temp_path.join("main.kdl"), main_content).unwrap();

        let result = resolve_includes(&temp_path.join("main.kdl"));

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("nonexistent.kdl"));
        // Should be a clear error about file not found
        assert!(
            err_msg.to_lowercase().contains("no such file")
                || err_msg.to_lowercase().contains("not found")
                || err_msg.to_lowercase().contains("failed to canonicalize")
        );
    }

    #[test]
    fn test_mixed_success_one_missing_file() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create a valid included file
        let valid_content = r#"
input {
    keyboard {
        repeat-rate 30
    }
}
"#;
        fs::write(temp_path.join("valid.kdl"), valid_content).unwrap();

        // Create main config that includes one valid and one missing file
        let main_content = r#"
include "valid.kdl"
include "missing.kdl"
layout {
    gaps 15
}
"#;
        fs::write(temp_path.join("main.kdl"), main_content).unwrap();

        let result = resolve_includes(&temp_path.join("main.kdl"));

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("missing.kdl"));

        // Should provide informative error about which file is missing
        assert!(
            err_msg.to_lowercase().contains("no such file")
                || err_msg.to_lowercase().contains("not found")
                || err_msg.to_lowercase().contains("failed to canonicalize")
        );
    }

    #[test]
    fn test_nested_missing_file_error() {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        let temp_path = temp_dir.path();

        // Create level1.kdl that tries to include a missing file
        let level1_content = r#"
include "missing_deep.kdl"
layout {
    gaps 25
}
"#;
        fs::write(temp_path.join("level1.kdl"), level1_content).unwrap();

        // Create main.kdl that includes level1.kdl
        let main_content = r#"
include "level1.kdl"
input {
    mouse {
        accel-speed 0.3
    }
}
"#;
        fs::write(temp_path.join("main.kdl"), main_content).unwrap();

        let result = resolve_includes(&temp_path.join("main.kdl"));

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("missing_deep.kdl"));

        // The error should mention the missing file
        assert!(
            err_msg.to_lowercase().contains("no such file")
                || err_msg.to_lowercase().contains("not found")
                || err_msg.to_lowercase().contains("failed to canonicalize")
        );
    }
}
