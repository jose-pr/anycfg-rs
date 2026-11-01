//! House rules a test holds: no `unsafe`, no process-wide state, no printing in the library.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files
}

/// The library's source with its unit-test modules, which may print, cut off.
fn library_lines() -> Vec<(PathBuf, usize, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut lines = Vec::new();
    for file in sources(&root) {
        let text = fs::read_to_string(&file).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap_or(&text);
        for (index, line) in code.lines().enumerate() {
            lines.push((file.clone(), index + 1, line.to_owned()));
        }
    }
    lines
}

fn assert_absent(what: &str, words: &[&str]) {
    let offenders: Vec<String> = library_lines()
        .into_iter()
        .filter(|(_, _, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, _, line)| {
            let line = line.replace("'static", "");
            words.iter().any(|word| line.contains(word))
        })
        .map(|(file, number, line)| format!("{}:{number}: {line}", file.display()))
        .collect();
    assert!(offenders.is_empty(), "{what}:\n{}", offenders.join("\n"));
}

#[test]
fn no_file_holds_unsafe_code() {
    assert_absent(
        "unsafe outside the crate's forbid",
        &["unsafe {", "unsafe fn", "unsafe impl"],
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    assert!(
        fs::read_to_string(root)
            .unwrap()
            .contains("#![forbid(unsafe_code)]")
    );
}

#[test]
fn the_library_keeps_no_process_wide_state() {
    assert_absent(
        "process-wide state",
        &["static ", "OnceLock", "LazyLock", "thread_local!"],
    );
}

#[test]
fn the_library_neither_prints_nor_exits_nor_changes_the_environment() {
    assert_absent(
        "printing, exiting or changing the process",
        &[
            "println!",
            "eprintln!",
            "print!(",
            "process::exit",
            "abort(",
            "set_var",
            "remove_var",
            "set_current_dir",
        ],
    );
}
