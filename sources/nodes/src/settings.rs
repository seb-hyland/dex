//! App-wide settings

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/**
    The editor command used when nothing has been set.

    `-n` opens the directory in a new window, which is the whole point rather
    than a nicety: a language server resolves imports from its *workspace root*,
    so a file opened into a project that is already open resolves nothing in the
    checkout — no completions and no errors, which looks exactly like the
    bindings being missing.
*/
pub const DEFAULT_EDITOR: &str = "zed -n $1 $2";

/// The directory to open, substituted for `$1`.
const DIR_PLACEHOLDER: &str = "$1";
/// The file to open, substituted for `$2`.
const FILE_PLACEHOLDER: &str = "$2";

fn venv_slot() -> &'static Mutex<Option<PathBuf>> {
    static VENV: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    VENV.get_or_init(|| Mutex::new(None))
}

fn editor_slot() -> &'static Mutex<String> {
    static EDITOR: OnceLock<Mutex<String>> = OnceLock::new();
    EDITOR.get_or_init(|| Mutex::new(DEFAULT_EDITOR.to_owned()))
}

/// The virtual environment currently on the interpreter's path, if any.
pub fn venv() -> Option<PathBuf> {
    venv_slot().lock().ok()?.clone()
}

/// Where a virtual environment keeps its installed packages.
pub fn site_packages(venv: &Path) -> Option<PathBuf> {
    // Windows keeps them directly under `Lib`; everyone else under the minor
    // version, which is the interpreter's to decide and not ours to guess.
    let windows = venv.join("Lib").join("site-packages");
    if windows.is_dir() {
        return Some(windows);
    }
    let lib = venv.join("lib");
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&lib)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("site-packages"))
        .filter(|path| path.is_dir())
        .collect();
    // Sorted, so a venv carrying two versions resolves the same way every time.
    candidates.sort();
    candidates.pop()
}

// Put `path`'s packages on the interpreter's import path, taking the previous environment's back off.
pub fn set_venv(path: Option<PathBuf>) -> Result<(), String> {
    let resolved = match &path {
        Some(dir) if !dir.is_dir() => return Err(format!("{} is not a directory", dir.display())),
        Some(dir) => Some(site_packages(dir).ok_or_else(|| {
            format!(
                "{} is not a virtual environment: nothing under lib/*/site-packages",
                dir.display()
            )
        })?),
        None => None,
    };

    let previous = venv().and_then(|old| site_packages(&old));
    rewrite_sys_path(previous.as_deref(), resolved.as_deref())?;

    let mut slot = venv_slot().lock().map_err(|_| "settings lock poisoned")?;
    *slot = path;
    Ok(())
}

/// Swap one entry for another at the front of `sys.path`.
fn rewrite_sys_path(remove: Option<&Path>, add: Option<&Path>) -> Result<(), String> {
    use pyo3::prelude::*;
    Python::attach(|py| -> PyResult<()> {
        let path = py.import("sys")?.getattr("path")?;
        if let Some(old) = remove {
            let old = old.to_string_lossy().into_owned();
            // `remove` raises when the entry is not there, which is not a
            // failure: something else may have rewritten the path.
            let _ = path.call_method1("remove", (old,));
        }
        if let Some(new) = add {
            // At the front, so the chosen environment wins over whatever the
            // embedded interpreter was built against.
            path.call_method1("insert", (0, new.to_string_lossy().into_owned()))?;
        }
        Ok(())
    })
    .map_err(|e: pyo3::PyErr| e.to_string())
}

/// The editor command template, as the user typed it.
pub fn editor_command() -> String {
    editor_slot()
        .lock()
        .map(|cmd| cmd.clone())
        .unwrap_or_else(|_| DEFAULT_EDITOR.to_owned())
}

/// Set the editor command template. Blank restores the default.
pub fn set_editor_command(command: String) {
    let command = if command.trim().is_empty() {
        DEFAULT_EDITOR.to_owned()
    } else {
        command
    };
    if let Ok(mut slot) = editor_slot().lock() {
        *slot = command;
    }
}

/**
    The argv for opening `file` inside `dir`, from a command template.
    `$1` is the directory and `$2` the file. A template that names neither gets both appended.
*/
pub fn editor_argv(template: &str, dir: &Path, file: &Path) -> Vec<String> {
    let (dir, file) = (
        dir.to_string_lossy().into_owned(),
        file.to_string_lossy().into_owned(),
    );
    let mut argv: Vec<String> = template
        .split_whitespace()
        .map(|token| {
            token
                .replace(DIR_PLACEHOLDER, &dir)
                .replace(FILE_PLACEHOLDER, &file)
        })
        .collect();

    if !template.contains(DIR_PLACEHOLDER) {
        argv.push(dir);
    }
    if !template.contains(FILE_PLACEHOLDER) {
        argv.push(file);
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(template: &str) -> Vec<String> {
        editor_argv(template, Path::new("/tmp/co"), Path::new("/tmp/co/main.py"))
    }

    #[test]
    fn a_template_naming_neither_gets_both_appended() {
        assert_eq!(argv("zed -n"), ["zed", "-n", "/tmp/co", "/tmp/co/main.py"]);
        assert_eq!(argv("code"), ["code", "/tmp/co", "/tmp/co/main.py"]);
    }

    #[test]
    fn a_template_names_where_each_goes() {
        assert_eq!(
            argv("zed -n $1 $2"),
            ["zed", "-n", "/tmp/co", "/tmp/co/main.py"]
        );
        // Order is the template's to choose, and either may be left out.
        assert_eq!(
            argv("edit $2 --root $1"),
            ["edit", "/tmp/co/main.py", "--root", "/tmp/co"]
        );
        assert_eq!(argv("edit $2"), ["edit", "/tmp/co/main.py", "/tmp/co"]);
        assert_eq!(argv("edit $1"), ["edit", "/tmp/co", "/tmp/co/main.py"]);
    }

    #[test]
    fn a_placeholder_inside_a_token_is_still_substituted() {
        assert_eq!(
            argv("edit --dir=$1 --file=$2"),
            ["edit", "--dir=/tmp/co", "--file=/tmp/co/main.py"]
        );
    }

    /// A path with a space in it is one argument, not two.
    #[test]
    fn a_spaced_path_stays_one_argument() {
        let argv = editor_argv(
            "zed -n $1 $2",
            Path::new("/tmp/my checkouts"),
            Path::new("/tmp/my checkouts/main.py"),
        );
        assert_eq!(
            argv,
            [
                "zed",
                "-n",
                "/tmp/my checkouts",
                "/tmp/my checkouts/main.py"
            ]
        );
    }

    /// A folder that is not an environment is not silently accepted.
    #[test]
    fn a_folder_without_site_packages_is_not_a_venv() {
        let dir = std::env::temp_dir().join("dex-venv-test-empty");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            site_packages(&dir).is_none(),
            "an empty folder is not a venv"
        );

        let sp = dir.join("lib").join("python3.99").join("site-packages");
        std::fs::create_dir_all(&sp).unwrap();
        assert_eq!(site_packages(&dir).as_deref(), Some(sp.as_path()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
