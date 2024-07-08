use std::fs;
use std::path::PathBuf;

const NO_TEST_COMMENT: &str = "<!-- no-test -->";

struct KdlCodeBlock {
    filename: String,
    code: String,
    line_number: usize,
}

fn extract_kdl_from_file(file_contents: &str, filename: &str) -> Vec<KdlCodeBlock> {
    // Removes the > from callouts that might contain ```kdl```
    let lines: Vec<_> = file_contents
        .lines()
        .map(|line| {
            let line = line.trim();
            if line.starts_with(">") {
                if line.len() == 1 {
                    ""
                } else {
                    &line[2..]
                }
            } else {
                line
            }
        })
        .enumerate()
        .collect();

    let mut lines_iter = lines.iter();
    let mut kdl_code_blocks = vec![];

    while let Some((line_number, line)) = lines_iter.next() {
        let start_snippet =
            line.starts_with("```kdl") && lines[line_number - 1].1 != NO_TEST_COMMENT;

        if !start_snippet {
            continue;
        }

        let mut snippet = String::new();
        for (_, line) in lines_iter
            .by_ref()
            .take_while(|(_, line)| !line.starts_with("```"))
        {
            snippet.push_str(line);
            snippet.push('\n');
        }

        kdl_code_blocks.push(KdlCodeBlock {
            code: snippet,
            line_number: *line_number,
            filename: filename.to_string(),
        });
    }

    kdl_code_blocks
}

#[test]
fn wiki_docs_parses() {
    let wiki_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wiki");

    let code_blocks = fs::read_dir(&wiki_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|ft| ft.is_file()))
        .filter(|file| {
            file.path()
                .extension()
                .map(|ext| ext == "md")
                .unwrap_or(false)
        })
        .map(|file| {
            let file_contents = fs::read_to_string(file.path()).unwrap();
            let file_path = file.path();
            let filename = file_path.to_str().unwrap();
            extract_kdl_from_file(&file_contents, filename)
        })
        .flatten();

    let mut errors = vec![];

    for KdlCodeBlock {
        code,
        line_number,
        filename,
    } in code_blocks
    {
        if let Err(error) = niri_config::Config::parse(&filename, &code) {
            errors.push(format!(
                "Error parsing wiki KDL code block at {}:{}: {:?}",
                filename,
                line_number,
                miette::Report::new(error)
            ))
        }
    }

    if !errors.is_empty() {
        panic!(
            "Errors parsing {} wiki KDL code blocks:\n{}",
            errors.len(),
            errors.join("\n")
        );
    }
}
