//! Editing a buffer in an external editor.

use std::path::{Path, PathBuf};

/// Marks a line as injected, so it can be removed again.
pub const MARKER: &str = "# DEX INJECTION. Do not edit or remove this comment.";

/// Modules a declared type may name, imported only when one does.
///
/// `dex` is always imported; these are the extras. Add to this when a value
/// kind starts declaring a type from somewhere new.
const HEADER_MODULES: &[&str] = &["typing"];

/// A language server config scoped to the checkout directory.
const PYRIGHT_CONFIG: &str = r#"{
  "include": ["."],
  "extraPaths": ["."],
  "typeCheckingMode": "basic",
  "reportMissingModuleSource": "none"
}
"#;

/**
    The editor used to open a checkout.

    Hardcoded for now; a user-configurable setting will replace it. `$DEX_EDITOR`
    overrides the program so tests do not launch anything.
*/
const EDITOR_PROGRAM: &str = "zed";

/**
    Flags that make the checkout the editor's project root.

    This is the whole point rather than a nicety: a language server resolves
    imports from its *workspace root*, so a file opened into a project that is
    already open resolves nothing in the checkout — no completions and no
    errors, which looks exactly like the bindings being missing. `-n` opens the
    directory in a new window, making it the root.
*/
const EDITOR_ARGS: &[&str] = &["-n"];

/// Where a buffer is checked out to, and what was last seen there.
#[derive(Clone, Debug)]
pub struct Checkout {
    pub dir: PathBuf,
    /// The buffer file the user edits.
    pub file: PathBuf,
    /// The contents as last written or read, so a change is detectable without
    /// trusting mtime resolution.
    pub last_seen: String,
}

impl Checkout {
    pub fn main_file(&self) -> &Path {
        &self.file
    }
}

/**
    Check `source` out under `key` and open it in the editor.

    `globals` are the names the runtime seeds into the script's namespace, as
    `(name, python type)` — a lambda passes its wired arguments, a prelude
    passes none.
*/
pub fn open(key: &str, source: &str, globals: &[(String, String)]) -> std::io::Result<Checkout> {
    let checkout = write(key, source, globals)?;
    open_in_editor(checkout.main_file())?;
    Ok(checkout)
}

/// Write a checkout without opening it.
pub fn write(key: &str, source: &str, globals: &[(String, String)]) -> std::io::Result<Checkout> {
    let dir = std::env::temp_dir().join(format!("dex-checkout-{key}"));
    std::fs::create_dir_all(&dir)?;

    // Rendered from the live bindings, so a checkout can never describe a stale
    // API and there is no generated file to keep in step.
    std::fs::write(dir.join("dex.pyi"), dex_core::stubs_gen::render())?;
    std::fs::write(dir.join("pyrightconfig.json"), PYRIGHT_CONFIG)?;

    let file = dir.join("main.py");
    let contents = with_injected(source, globals);
    std::fs::write(&file, &contents)?;

    Ok(Checkout {
        dir,
        file,
        last_seen: contents,
    })
}

/// What a poll found.
pub struct Pulled {
    /// The edited source, with the injected header removed.
    pub source: String,
    /// The checkout, with `last_seen` advanced.
    pub checkout: Checkout,
}

/**
    Read a checkout, returning the edit if there was one.

    [`None`] when nothing changed, or when the file cannot be read — a moved or
    deleted file leaves the checkout in place so it can be reopened, rather than
    silently discarding the user's work.
*/
pub fn poll(checkout: &Checkout) -> Option<Pulled> {
    let contents = std::fs::read_to_string(&checkout.file).ok()?;
    if contents == checkout.last_seen {
        return None;
    }
    Some(Pulled {
        source: strip_injected(&contents),
        checkout: Checkout {
            last_seen: contents,
            ..checkout.clone()
        },
    })
}

/**
    The header a checked-out file opens with.

    Imports whatever the declared types name. The header sits in the script's
    own namespace, so an unqualified type would simply be an undefined name —
    callers pass fully qualified types (`dex.NodeUid`, `typing.Any`) and the
    imports are derived from them.
*/
fn header(globals: &[(String, String)]) -> Vec<String> {
    let mut lines = vec![format!("import dex  {MARKER}")];

    // Only pull in modules the declarations actually mention, so the header
    // does not carry an unused import for every script.
    for module in HEADER_MODULES {
        if globals
            .iter()
            .any(|(_, ty)| ty.contains(&format!("{module}.")))
        {
            lines.push(format!("import {module}  {MARKER}"));
        }
    }

    for (name, ty) in globals {
        lines.push(format!("{name}: {ty}  {MARKER}"));
    }
    lines
}

/// Add the injected header, replacing any already present.
pub fn with_injected(source: &str, globals: &[(String, String)]) -> String {
    let body = strip_injected(source);
    let mut out = header(globals).join("\n");
    out.push_str("\n\n");
    out.push_str(&body);
    out
}

/**
    Remove the injected header, so what runs is what the node holds.

    Only the leading run of marked lines goes. A line the user wrote further
    down survives even if it carries the same marker.
*/
pub fn strip_injected(contents: &str) -> String {
    let lines: Vec<&str> = contents.lines().collect();
    let mut start = 0;
    while lines.get(start).is_some_and(|l| is_injected(l)) {
        start += 1;
    }
    if start == 0 {
        return contents.to_owned();
    }
    // Drop one blank line after the header, so a round trip is stable rather
    // than accumulating blank lines.
    if lines.get(start).is_some_and(|l| l.trim().is_empty()) {
        start += 1;
    }

    let mut out = lines[start..].join("\n");
    if contents.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}

fn is_injected(line: &str) -> bool {
    line.trim_end().ends_with(MARKER)
}

/// Open the checkout containing `path` as its own project, with `path` open.
pub fn open_in_editor(path: &Path) -> std::io::Result<()> {
    use std::process::Command;

    let dir = path.parent().unwrap_or(path);
    let program = std::env::var("DEX_EDITOR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| EDITOR_PROGRAM.to_owned());

    // The directory first so it becomes the worktree, then the file to open.
    Command::new(&program)
        .args(EDITOR_ARGS)
        .arg(dir)
        .arg(path)
        .current_dir(dir)
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> Vec<(String, String)> {
        vec![
            ("count".to_owned(), "int".to_owned()),
            ("label".to_owned(), "str".to_owned()),
        ]
    }

    #[test]
    fn the_header_round_trips() {
        let source = "def transform():\n    return dex.Label.new('hi')\n";
        let checked_out = with_injected(source, &args());
        assert!(checked_out.starts_with("import dex"));
        assert!(checked_out.contains("count: int"));
        assert_eq!(strip_injected(&checked_out), source);
    }

    #[test]
    fn reinjecting_replaces_the_header_rather_than_stacking() {
        let once = with_injected("x = 1\n", &args());
        // Reopening with different arguments refreshes the header.
        let twice = with_injected(&once, &[("other".to_owned(), "bool".to_owned())]);
        assert!(twice.contains("other: bool"));
        assert!(!twice.contains("count: int"));
        assert_eq!(twice.matches("import dex").count(), 1);
        assert_eq!(strip_injected(&twice), "x = 1\n");
    }

    #[test]
    fn a_file_without_a_header_is_untouched() {
        let plain = "import os\n\ndef transform():\n    pass\n";
        assert_eq!(strip_injected(plain), plain);
    }

    #[test]
    fn only_the_leading_header_is_removed() {
        // The user's own line carrying the marker must survive.
        let source = format!("def transform():\n    y = 1  {MARKER}\n    pass\n");
        let checked_out = with_injected(&source, &args());
        assert_eq!(strip_injected(&checked_out), source);
    }

    #[test]
    fn no_globals_still_gets_the_import() {
        let checked_out = with_injected("pass\n", &[]);
        assert!(checked_out.starts_with("import dex"));
        assert_eq!(strip_injected(&checked_out), "pass\n");
    }
}
