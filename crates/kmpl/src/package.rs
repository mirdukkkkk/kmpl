//! The commands that change a project's dependencies.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::loader;
use crate::manifest::{self, Manifest};
use crate::remote::{self, Fetch, Remote};

/// `kmpl add <source> [name]`, run in the directory `dir`. Nothing is written
/// to `kmpl.toml` until the module and everything it imports have been found.
/// Returns the line to show.
pub fn add(
    dir: &Path,
    source: &str,
    name: Option<&str>,
    cache: Option<PathBuf>,
    fetch: Fetch<'_>,
) -> Result<String, String> {
    check_source(source)?;
    let name = match name {
        Some(name) => name.to_owned(),
        None => match remote::repository_name(source)? {
            Some(name) => name.to_owned(),
            None => return Err(format!("give `{source}` a name: kmpl add {source} <name>")),
        },
    };
    if !is_valid_name(&name) {
        return Err(format!(
            "`{name}` cannot be a dependency name, use letters, digits, `_` and `-`: \
             kmpl add {source} <name>"
        ));
    }

    let found = manifest::find(dir)?;
    if let Some(project) = &found
        && let Some(existing) = project.dependencies.get(&name)
    {
        return Err(format!("`{name}` is already a dependency: `{existing}`"));
    }

    let (project_dir, known) = match found {
        Some(project) => (project.dir, project.dependencies),
        None => (
            std::fs::canonicalize(dir).map_err(cannot_read(dir))?,
            HashMap::new(),
        ),
    };
    let mut dependencies = known;
    dependencies.insert(name.clone(), source.to_owned());
    let project = Manifest {
        dir: project_dir.clone(),
        dependencies,
    };
    let remote = open_lock(&project_dir, cache, fetch)?;
    loader::fetch_dependencies(project, std::slice::from_ref(&name), remote, fetch)?.save()?;

    let file = project_dir.join(manifest::FILE_NAME);
    let text = match file.is_file() {
        true => std::fs::read_to_string(&file).map_err(cannot_read(&file))?,
        false => manifest::EMPTY.to_owned(),
    };
    std::fs::write(&file, manifest::with_dependency(&text, &name, source))
        .map_err(|e| format!("cannot write `{}`: {e}", file.display()))?;
    Ok(format!("added `{name}` from `{source}`"))
}

/// What `kmpl update` was asked to do.
#[derive(Debug, Clone, Copy, Default)]
pub struct Update<'a> {
    /// One dependency, or else every link in `kmpl.lock`.
    pub name: Option<&'a str>,
    /// The tag, branch or commit `name` should move to.
    pub version: Option<&'a str>,
    /// Only report what would change.
    pub dry_run: bool,
}

/// `kmpl update`, run in the directory `dir`: finds again what links point to.
/// Files are written only if everything succeeded. Returns the lines to show.
pub fn update(
    dir: &Path,
    request: &Update<'_>,
    cache: Option<PathBuf>,
    fetch: Fetch<'_>,
) -> Result<Vec<String>, String> {
    let mut project = project_in(dir)?;
    let mut remote = open_lock(&project.dir, cache, fetch)?;
    let before = remote.pins();
    let mut moved = None;

    let names: Vec<String> = match request.name {
        None => {
            remote.refresh(None)?;
            let mut names: Vec<String> = project.dependencies.keys().cloned().collect();
            names.sort();
            names
        }
        Some(name) => {
            let Some(old) = project.dependencies.get(name).cloned() else {
                return Err(format!(
                    "`{name}` is not listed in `{}`",
                    manifest::FILE_NAME
                ));
            };
            if !remote::is_link(&old) {
                return Err(format!(
                    "`{name}` is a local path, there is nothing to update"
                ));
            }
            let link = match request.version {
                None => old,
                Some(version) => {
                    let new = moved_link(name, &old, version)?;
                    let others: Vec<&str> = project
                        .dependencies
                        .iter()
                        .filter(|(other, _)| other.as_str() != name)
                        .map(|(_, value)| value.as_str())
                        .collect();
                    remote.forget(&old, &others)?;
                    project.dependencies.insert(name.to_owned(), new.clone());
                    if new != old {
                        moved = Some((old, new.clone()));
                    }
                    new
                }
            };
            remote.refresh(Some(&link))?;
            vec![name.to_owned()]
        }
    };

    let project_dir = project.dir.clone();
    let remote = loader::fetch_dependencies(project, &names, remote, fetch)?;

    let mut lines = Vec::new();
    if let Some((old, new)) = &moved {
        lines.push(format!("{old} -> {new}"));
    }
    lines.extend(remote.changes_since(&before));
    if lines.is_empty() {
        lines.push("everything is up to date".to_owned());
    } else if request.dry_run {
        lines.push("dry run: nothing was written".to_owned());
    }

    if !request.dry_run {
        if let (Some((_, new)), Some(name)) = (&moved, request.name) {
            let file = project_dir.join(manifest::FILE_NAME);
            let text = std::fs::read_to_string(&file).map_err(cannot_read(&file))?;
            std::fs::write(&file, manifest::with_changed_dependency(&text, name, new))
                .map_err(|e| format!("cannot write `{}`: {e}", file.display()))?;
        }
        remote.save()?;
    }
    Ok(lines)
}

/// The link of `name` at another version.
fn moved_link(name: &str, link: &str, version: &str) -> Result<String, String> {
    if version.is_empty()
        || version.contains('/')
        || version.contains('"')
        || version.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(format!("`{version}` is not a tag, branch or commit"));
    }
    remote::with_reference(link, version)?.ok_or_else(|| {
        format!("`{name}` is not a GitHub link, so it has no version to choose: `{link}`")
    })
}

/// `kmpl vendor`, run in the directory `dir`: puts every file of `kmpl.lock`
/// into the project. Returns the line to show.
pub fn vendor(dir: &Path, cache: Option<PathBuf>, fetch: Fetch<'_>) -> Result<String, String> {
    let project = project_in(dir)?;
    let mut names: Vec<String> = project.dependencies.keys().cloned().collect();
    names.sort();

    let remote = open_lock(&project.dir, cache, fetch)?;
    let mut remote = loader::fetch_dependencies(project, &names, remote, fetch)?;
    remote.save()?;
    let count = remote.fill_vendor()?;
    let word = if count == 1 { "file" } else { "files" };
    Ok(format!("{count} {word} in `{}`", remote::VENDOR_DIR))
}

/// The project that `dir` belongs to. Without a `kmpl.toml` it is `dir` itself.
fn project_in(dir: &Path) -> Result<Manifest, String> {
    Ok(match manifest::find(dir)? {
        Some(project) => project,
        None => Manifest {
            dir: std::fs::canonicalize(dir).map_err(cannot_read(dir))?,
            dependencies: HashMap::new(),
        },
    })
}

fn open_lock<'f>(
    dir: &Path,
    cache: Option<PathBuf>,
    fetch: Fetch<'f>,
) -> Result<Remote<'f>, String> {
    Remote::open(remote::lock_path(Some(dir), dir), false, cache, fetch)
}

fn cannot_read(path: &Path) -> impl FnOnce(std::io::Error) -> String + '_ {
    move |e| format!("cannot read `{}`: {e}", path.display())
}

/// Refuses what could not be read back from `kmpl.toml`, and other schemes.
fn check_source(source: &str) -> Result<(), String> {
    if source.contains('"') || source.chars().any(char::is_control) {
        return Err(format!("`{source}` is not a link or a path"));
    }
    if source.contains("://") && !remote::is_link(source) {
        return Err(format!(
            "only `https://` links can be added, not `{source}`"
        ));
    }
    Ok(())
}

/// What can follow `from` as a plain name and be a key in `kmpl.toml`.
fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const COMMIT: &str = "4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kmpl-add-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn github(url: &str) -> Result<Option<Vec<u8>>, String> {
        let raw = format!("https://raw.githubusercontent.com/u/my-lib/{COMMIT}");
        let refs = "https://github.com/u/my-lib.git/info/refs?service=git-upload-pack";
        let text = if url == refs {
            let line = format!("{COMMIT} refs/tags/v1\0caps\n");
            format!("{:04x}{line}0000", line.len() + 4)
        } else if url == format!("{raw}/mod.kmpl") {
            "import wire from \"./wire.kmpl\";\nconst N: int = wire.M;\n".to_owned()
        } else if url == format!("{raw}/wire.kmpl") {
            "const M: int = 3;\n".to_owned()
        } else {
            return Ok(None);
        };
        Ok(Some(text.into_bytes()))
    }

    const LINK: &str = "https://github.com/u/my-lib/v1";

    #[test]
    fn the_name_comes_from_the_repository_and_everything_is_locked() {
        let dir = temp("github");
        let line = add(&dir, LINK, None, None, &github).unwrap();
        assert_eq!(line, format!("added `my-lib` from `{LINK}`"));

        let toml = std::fs::read_to_string(dir.join("kmpl.toml")).unwrap();
        assert_eq!(
            toml,
            format!("version = 1\n\n[dependencies]\nmy-lib = \"{LINK}\"\n")
        );
        let lock = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();
        assert!(lock.contains(&format!("{LINK} commit:{COMMIT}")), "{lock}");
        assert!(lock.contains("/mod.kmpl sha256:"), "{lock}");
        assert!(lock.contains("/wire.kmpl sha256:"), "{lock}");
    }

    #[test]
    fn a_given_name_replaces_the_repository_name() {
        let dir = temp("named");
        add(&dir, LINK, Some("pb"), None, &github).unwrap();
        let toml = std::fs::read_to_string(dir.join("kmpl.toml")).unwrap();
        assert!(toml.contains(&format!("pb = \"{LINK}\"")), "{toml}");
    }

    #[test]
    fn other_sources_need_a_name() {
        let dir = temp("no_name");
        let error = add(&dir, "https://h.org/a.kmpl", None, None, &github).unwrap_err();
        assert!(
            error.contains("give `https://h.org/a.kmpl` a name"),
            "{error}"
        );
        let error = add(&dir, "./lib", None, None, &github).unwrap_err();
        assert!(error.contains("give `./lib` a name"), "{error}");
    }

    #[test]
    fn what_does_not_exist_is_not_added() {
        let dir = temp("missing");
        let error = add(&dir, "https://github.com/u/my-lib/v9", None, None, &github).unwrap_err();
        assert_eq!(
            error,
            "no tag or branch `v9` in `https://github.com/u/my-lib/v9`"
        );
        let error = add(&dir, "https://h.org/a.kmpl", Some("a"), None, &github).unwrap_err();
        assert_eq!(error, "`https://h.org/a.kmpl` does not exist");
        assert!(!dir.join("kmpl.toml").exists());
        assert!(!dir.join("kmpl.lock").exists());
    }

    #[test]
    fn a_local_module_is_added_from_the_directory_of_the_manifest() {
        let dir = temp("local");
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("lib/mod.kmpl"), "const N: int = 1;\n").unwrap();
        std::fs::write(dir.join("kmpl.toml"), "version = 1\n").unwrap();

        add(&dir.join("src"), "./lib", Some("lib"), None, &github).unwrap();
        let toml = std::fs::read_to_string(dir.join("kmpl.toml")).unwrap();
        assert_eq!(toml, "version = 1\n\n[dependencies]\nlib = \"./lib\"\n");
        assert!(!dir.join("src/kmpl.toml").exists());
    }

    #[test]
    fn a_name_is_not_taken_twice_and_must_be_valid() {
        let dir = temp("twice");
        add(&dir, LINK, Some("pb"), None, &github).unwrap();
        let error = add(&dir, LINK, Some("pb"), None, &github).unwrap_err();
        assert_eq!(error, format!("`pb` is already a dependency: `{LINK}`"));

        for name in ["", "1a", "a.b", "a/b", "a b", "a\"b"] {
            let error = add(&dir, LINK, Some(name), None, &github).unwrap_err();
            assert!(error.contains("cannot be a dependency name"), "{error}");
        }
    }

    #[test]
    fn broken_or_foreign_sources_are_refused() {
        let dir = temp("bad_source");
        let error = add(&dir, "http://h.org/a", Some("a"), None, &github).unwrap_err();
        assert!(error.contains("only `https://`"), "{error}");
        let error = add(&dir, "./a\"b", Some("a"), None, &github).unwrap_err();
        assert!(error.contains("not a link or a path"), "{error}");
    }

    /// A repository whose tag `v1` is at `commit`, and a plain file.
    fn serving<'a>(
        commit: &'a str,
        wire: &'a str,
    ) -> impl Fn(&str) -> Result<Option<Vec<u8>>, String> + 'a {
        move |url: &str| {
            let raw = format!("https://raw.githubusercontent.com/u/my-lib/{commit}");
            let refs = "https://github.com/u/my-lib.git/info/refs?service=git-upload-pack";
            let text = if url == refs {
                let line = format!("{commit} refs/tags/v1\0caps\n");
                format!("{:04x}{line}0000", line.len() + 4)
            } else if url == format!("{raw}/mod.kmpl") {
                "const N: int = 1;\n".to_owned()
            } else if url == "https://h.org/a.kmpl" {
                wire.to_owned()
            } else {
                return Ok(None);
            };
            Ok(Some(text.into_bytes()))
        }
    }

    fn named(name: &str) -> Update<'_> {
        Update {
            name: Some(name),
            ..Update::default()
        }
    }

    const NEWER: &str = "1111111111111111111111111111111111111111";

    fn locked_project(name: &str) -> PathBuf {
        let dir = temp(name);
        let fetch = serving(COMMIT, "const A: int = 1;");
        add(&dir, LINK, None, None, &fetch).unwrap();
        add(&dir, "https://h.org/a.kmpl", Some("plain"), None, &fetch).unwrap();
        dir
    }

    #[test]
    fn update_moves_the_commit_and_drops_the_old_files() {
        let dir = locked_project("update");
        let fetch = serving(NEWER, "const A: int = 1;");
        let lines = update(&dir, &Update::default(), None, &fetch).unwrap();
        assert_eq!(lines, vec![format!("{LINK}: 4f2a4f2 -> 1111111")]);

        let lock = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();
        assert!(lock.contains(&format!("{LINK} commit:{NEWER}")), "{lock}");
        assert!(
            lock.contains(&format!("/{NEWER}/mod.kmpl sha256:")),
            "{lock}"
        );
        assert!(!lock.contains(&format!("/{COMMIT}/")), "{lock}");
    }

    #[test]
    fn update_accepts_a_plain_file_that_changed() {
        let dir = locked_project("plain");
        let fetch = serving(COMMIT, "const A: int = 2;");
        let lines = update(&dir, &Update::default(), None, &fetch).unwrap();
        assert_eq!(lines, vec!["https://h.org/a.kmpl: the file changed"]);
    }

    #[test]
    fn update_with_a_name_leaves_the_rest_alone() {
        let dir = locked_project("named_update");
        let fetch = serving(NEWER, "const A: int = 2;");
        let lines = update(&dir, &named("plain"), None, &fetch).unwrap();
        assert_eq!(lines, vec!["https://h.org/a.kmpl: the file changed"]);
        let lock = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();
        assert!(lock.contains(&format!("{LINK} commit:{COMMIT}")), "{lock}");
    }

    #[test]
    fn nothing_new_is_reported_as_such() {
        let dir = locked_project("same");
        let fetch = serving(COMMIT, "const A: int = 1;");
        let lines = update(&dir, &Update::default(), None, &fetch).unwrap();
        assert_eq!(lines, vec!["everything is up to date"]);
    }

    #[test]
    fn a_failed_update_leaves_the_lock_file_as_it_was() {
        let dir = locked_project("failed");
        let before = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();
        // The tag moved, but the module is gone at the new commit.
        let moved = serving(NEWER, "const A: int = 1;");
        let fetch = |url: &str| match url.contains("/mod.kmpl") {
            true => Ok(None),
            false => moved(url),
        };
        let error = update(&dir, &Update::default(), None, &fetch).unwrap_err();
        assert!(error.contains("does not exist"), "{error}");
        assert_eq!(
            std::fs::read_to_string(dir.join("kmpl.lock")).unwrap(),
            before
        );
    }

    #[test]
    fn update_refuses_unknown_names_and_local_paths() {
        let dir = temp("update_names");
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib/mod.kmpl"), "const N: int = 1;\n").unwrap();
        let fetch = serving(COMMIT, "const A: int = 1;");
        add(&dir, "./lib", Some("lib"), None, &fetch).unwrap();

        let error = update(&dir, &named("nope"), None, &fetch).unwrap_err();
        assert!(error.contains("not listed"), "{error}");
        let error = update(&dir, &named("lib"), None, &fetch).unwrap_err();
        assert!(error.contains("local path"), "{error}");
    }

    fn offline(url: &str) -> Result<Option<Vec<u8>>, String> {
        Err(format!("no network for `{url}`"))
    }

    #[test]
    fn a_vendored_project_needs_no_network_and_no_cache() {
        let dir = locked_project("vendor");
        let line = vendor(&dir, None, &serving(COMMIT, "const A: int = 1;")).unwrap();
        assert_eq!(line, "2 files in `vendor`");

        let single = temp("one_file");
        let fetch = serving(COMMIT, "const A: int = 1;");
        add(&single, LINK, None, None, &fetch).unwrap();
        assert_eq!(vendor(&single, None, &fetch).unwrap(), "1 file in `vendor`");

        let lock = dir.join("kmpl.lock");
        let mut remote = Remote::open(lock, true, None, &offline).unwrap();
        let url = remote.file_url(LINK).unwrap();
        assert_eq!(remote.read(&url).unwrap(), "const N: int = 1;\n");
        assert_eq!(
            remote.read("https://h.org/a.kmpl").unwrap(),
            "const A: int = 1;"
        );
    }

    #[test]
    fn vendor_drops_what_the_lock_file_no_longer_has() {
        let dir = locked_project("stale");
        vendor(&dir, None, &serving(COMMIT, "const A: int = 1;")).unwrap();
        let before = std::fs::read_dir(dir.join("vendor")).unwrap().count();
        std::fs::write(dir.join("vendor/README"), "mine").unwrap();

        let fetch = serving(COMMIT, "const A: int = 2;");
        update(&dir, &named("plain"), None, &fetch).unwrap();
        vendor(&dir, None, &fetch).unwrap();

        // One file was replaced by another, and a foreign file stays.
        let after = std::fs::read_dir(dir.join("vendor")).unwrap().count();
        assert_eq!(after, before + 1);
        assert!(dir.join("vendor/README").exists());
    }

    #[test]
    fn a_damaged_vendored_file_is_not_trusted() {
        let dir = locked_project("damaged");
        vendor(&dir, None, &serving(COMMIT, "const A: int = 1;")).unwrap();
        for entry in std::fs::read_dir(dir.join("vendor")).unwrap() {
            std::fs::write(entry.unwrap().path(), "evil").unwrap();
        }
        let lock = dir.join("kmpl.lock");
        let mut remote = Remote::open(lock, true, None, &offline).unwrap();
        let error = remote.read("https://h.org/a.kmpl").unwrap_err();
        assert!(error.contains("no network"), "{error}");
    }

    /// Tags `v1` at `COMMIT` and `v2` at `NEWER`, with a module at both.
    fn two_tags(url: &str) -> Result<Option<Vec<u8>>, String> {
        if url.ends_with("info/refs?service=git-upload-pack") {
            let first = format!("{COMMIT} refs/tags/v1\0caps\n");
            let second = format!("{NEWER} refs/tags/v2\n");
            let text = format!(
                "{:04x}{first}{:04x}{second}0000",
                first.len() + 4,
                second.len() + 4
            );
            return Ok(Some(text.into_bytes()));
        }
        Ok(url
            .ends_with("/mod.kmpl")
            .then(|| b"const N: int = 1;\n".to_vec()))
    }

    fn to_version<'a>(name: &'a str, version: &'a str) -> Update<'a> {
        Update {
            version: Some(version),
            ..named(name)
        }
    }

    #[test]
    fn update_moves_a_dependency_to_a_version() {
        let dir = temp("version");
        add(&dir, LINK, Some("pb"), None, &two_tags).unwrap();
        let lines = update(&dir, &to_version("pb", "v2"), None, &two_tags).unwrap();
        let new = "https://github.com/u/my-lib/v2";
        assert_eq!(lines, vec![format!("{LINK} -> {new}")]);

        let toml = std::fs::read_to_string(dir.join("kmpl.toml")).unwrap();
        assert!(toml.contains(&format!("pb = \"{new}\"")), "{toml}");
        let lock = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();
        assert!(lock.contains(&format!("{new} commit:{NEWER}")), "{lock}");
        assert!(!lock.contains(LINK), "{lock}");
        assert!(!lock.contains(COMMIT), "{lock}");
    }

    #[test]
    fn a_version_that_does_not_exist_changes_nothing() {
        let dir = temp("no_version");
        add(&dir, LINK, Some("pb"), None, &two_tags).unwrap();
        let toml = std::fs::read_to_string(dir.join("kmpl.toml")).unwrap();
        let lock = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();

        let error = update(&dir, &to_version("pb", "v9"), None, &two_tags).unwrap_err();
        assert!(error.contains("no tag or branch `v9`"), "{error}");
        assert_eq!(
            std::fs::read_to_string(dir.join("kmpl.toml")).unwrap(),
            toml
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("kmpl.lock")).unwrap(),
            lock
        );
    }

    #[test]
    fn a_version_needs_a_github_link() {
        let dir = locked_project("version_plain");
        let fetch = serving(COMMIT, "const A: int = 1;");
        let error = update(&dir, &to_version("plain", "v2"), None, &fetch).unwrap_err();
        assert!(error.contains("not a GitHub link"), "{error}");
        let error = update(&dir, &to_version("my-lib", "a/b"), None, &fetch).unwrap_err();
        assert!(error.contains("not a tag, branch or commit"), "{error}");
    }

    #[test]
    fn a_dry_run_reports_and_writes_nothing() {
        let dir = temp("dry");
        add(&dir, LINK, Some("pb"), None, &two_tags).unwrap();
        let toml = std::fs::read_to_string(dir.join("kmpl.toml")).unwrap();
        let lock = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();

        let request = Update {
            dry_run: true,
            ..to_version("pb", "v2")
        };
        let lines = update(&dir, &request, None, &two_tags).unwrap();
        assert_eq!(lines[1], "dry run: nothing was written");
        assert!(lines[0].ends_with("/v2"), "{lines:?}");
        assert_eq!(
            std::fs::read_to_string(dir.join("kmpl.toml")).unwrap(),
            toml
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("kmpl.lock")).unwrap(),
            lock
        );
    }

    #[test]
    fn another_dependency_keeps_the_tag_it_shares() {
        let dir = temp("shared");
        add(&dir, LINK, Some("a"), None, &two_tags).unwrap();
        add(&dir, LINK, Some("b"), None, &two_tags).unwrap();
        update(&dir, &to_version("a", "v2"), None, &two_tags).unwrap();
        let lock = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();
        assert!(lock.contains(&format!("{LINK} commit:{COMMIT}")), "{lock}");
        assert!(
            lock.contains(&format!("/{COMMIT}/mod.kmpl sha256:")),
            "{lock}"
        );
    }
}
