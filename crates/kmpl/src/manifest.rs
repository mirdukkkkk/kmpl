//! `kmpl.toml`: the names a project gives to its dependencies.
//!
//! ```toml
//! version = 1
//!
//! [dependencies]
//! helpers = "../helpers"
//! pb = "https://github.com/user/pb/v1.2.0"
//! ```
//!
//! Only this much of TOML is understood, so no parser is needed for it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "kmpl.toml";

/// The version of the format of `kmpl.toml` this compiler reads.
const FORMAT_VERSION: &str = "1";

/// What a new `kmpl.toml` starts from.
pub const EMPTY: &str = "version = 1\n";

#[derive(Debug, PartialEq)]
pub struct Manifest {
    /// The directory of the file: local dependencies start from it.
    pub dir: PathBuf,
    pub dependencies: HashMap<String, String>,
}

/// Looks for `kmpl.toml` in `start` and then in the directories above it.
/// The error is ready to be shown.
pub fn find(start: &Path) -> Result<Option<Manifest>, String> {
    let start = if start.as_os_str().is_empty() {
        Path::new(".")
    } else {
        start
    };
    let absolute = std::fs::canonicalize(start)
        .map_err(|e| format!("cannot read `{}`: {e}", start.display()))?;

    for dir in absolute.ancestors() {
        let file = dir.join(FILE_NAME);
        if file.is_file() {
            let text = std::fs::read_to_string(&file)
                .map_err(|e| format!("cannot read `{}`: {e}", file.display()))?;
            let dependencies = parse(&text)
                .map_err(|(line, message)| format!("{}:{line}: {message}", file.display()))?;
            return Ok(Some(Manifest {
                dir: dir.to_path_buf(),
                dependencies,
            }));
        }
    }
    Ok(None)
}

/// `text` with one more dependency. Everything already there stays as it was,
/// comments included. The name must not be listed yet.
pub fn with_dependency(text: &str, name: &str, value: &str) -> String {
    let entry = format!("{name} = \"{value}\"");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let is_section = |line: &String| strip_comment(line).trim() == "[dependencies]";

    match lines.iter().position(is_section) {
        Some(section) => {
            // After the last dependency, so that comments below it stay below.
            let last = (section + 1..lines.len())
                .rev()
                .find(|index| !strip_comment(&lines[*index]).trim().is_empty())
                .unwrap_or(section);
            lines.insert(last + 1, entry);
        }
        None => {
            if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push("[dependencies]".to_owned());
            lines.push(entry);
        }
    }
    lines.join("\n") + "\n"
}

/// `text` with the value of the dependency `name` replaced. The rest of its
/// line stays, comment included. `text` must list it.
pub fn with_changed_dependency(text: &str, name: &str, value: &str) -> String {
    let mut in_dependencies = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        let code = strip_comment(line);
        let mut changed = None;
        if code.trim() == "[dependencies]" {
            in_dependencies = true;
        } else if in_dependencies
            && code
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == name)
            && let (Some(first), Some(last)) = (code.find('"'), code.rfind('"'))
        {
            changed = Some(format!("{}{value}{}", &line[..=first], &line[last..]));
        }
        lines.push(changed.unwrap_or_else(|| line.to_owned()));
    }
    lines.join("\n") + "\n"
}

/// The dependencies, or the number of the bad line and what is wrong with it.
fn parse(text: &str) -> Result<HashMap<String, String>, (usize, String)> {
    let mut version = None;
    let mut in_dependencies = false;
    let mut dependencies = HashMap::new();

    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = strip_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if section != "dependencies" {
                return Err((number, format!("unknown section `[{section}]`")));
            }
            in_dependencies = true;
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err((number, "expected `name = value`".to_owned()));
        };
        let (key, value) = (key.trim(), value.trim());

        if in_dependencies {
            let Some(value) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
                return Err((number, format!("the value of `{key}` must be in quotes")));
            };
            if dependencies
                .insert(key.to_owned(), value.to_owned())
                .is_some()
            {
                return Err((number, format!("`{key}` is listed twice")));
            }
        } else if key == "version" {
            if value != FORMAT_VERSION {
                return Err((
                    number,
                    format!(
                        "unsupported version {value}, this kmpl reads version {FORMAT_VERSION}"
                    ),
                ));
            }
            version = Some(number);
        } else {
            return Err((number, format!("unknown setting `{key}`")));
        }
    }

    if version.is_none() {
        return Err((1, format!("`version = {FORMAT_VERSION}` is required")));
    }
    Ok(dependencies)
}

/// Drops a `#` comment. A `#` inside quotes is part of the value.
fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (position, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..position],
            _ => {}
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::{parse, with_changed_dependency, with_dependency};

    #[test]
    fn a_changed_dependency_keeps_the_rest_of_its_line() {
        let text = "version = 1\n[dependencies]\nab = \"./x\"  # note\na = \"./y\" # \"quoted\"\n";
        assert_eq!(
            with_changed_dependency(text, "a", "./z"),
            "version = 1\n[dependencies]\nab = \"./x\"  # note\na = \"./z\" # \"quoted\"\n"
        );
    }

    #[test]
    fn a_new_dependency_goes_after_the_others_and_keeps_comments() {
        let text =
            "# project\nversion = 1\n\n[dependencies]\n# first\na = \"./a\"  # local\n\n# end\n";
        assert_eq!(
            with_dependency(text, "b", "./b"),
            "# project\nversion = 1\n\n[dependencies]\n# first\na = \"./a\"  # local\nb = \"./b\"\n\n# end\n"
        );
    }

    #[test]
    fn a_new_dependency_can_be_the_first() {
        assert_eq!(
            with_dependency("version = 1\n", "a", "./a"),
            "version = 1\n\n[dependencies]\na = \"./a\"\n"
        );
        assert_eq!(
            with_dependency("version = 1\n[dependencies]\n", "a", "./a"),
            "version = 1\n[dependencies]\na = \"./a\"\n"
        );
    }

    #[test]
    fn reads_dependencies() {
        let text = "# project\nversion = 1\n\n[dependencies]\na = \"./a\"  # local\nb = \"https://x/y#z\"\n";
        let deps = parse(text).unwrap();
        assert_eq!(deps["a"], "./a");
        assert_eq!(deps["b"], "https://x/y#z");
    }

    #[test]
    fn dependencies_are_optional() {
        assert!(parse("version = 1\n").unwrap().is_empty());
    }

    fn error(text: &str) -> (usize, String) {
        parse(text).unwrap_err()
    }

    #[test]
    fn the_version_is_required() {
        assert_eq!(
            error("[dependencies]\n"),
            (1, "`version = 1` is required".to_owned())
        );
        assert_eq!(
            error("version = 2\n"),
            (
                1,
                "unsupported version 2, this kmpl reads version 1".to_owned()
            )
        );
    }

    #[test]
    fn mistakes_name_their_line() {
        assert_eq!(error("version = 1\n[nope]\n").0, 2);
        assert_eq!(
            error("version = 1\nname = \"x\"\n"),
            (2, "unknown setting `name`".to_owned())
        );
        assert_eq!(
            error("version = 1\n[dependencies]\na = b\n").1,
            "the value of `a` must be in quotes"
        );
        assert_eq!(
            error("version = 1\n[dependencies]\na = \"x\"\na = \"y\"\n"),
            (4, "`a` is listed twice".to_owned())
        );
        assert_eq!(
            error("version = 1\nnothing\n"),
            (2, "expected `name = value`".to_owned())
        );
    }
}
