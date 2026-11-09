//! Reads the file given on the command line and everything it imports.
//!
//! `import x from "./a/b";` reads `a/b.kmpl` or `a/b/mod.kmpl` next to the
//! importing file. A module is known by its path from the directory of the
//! first file (`a.b`), so a file imported from several places is read once.
//! Modules are listed after the ones they import.
//!
//! `"std:fs"` is a library that is part of `kmpl` itself. A plain name like
//! `"pb"` is looked up in `kmpl.toml`. A link is downloaded, see `remote`.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use kmpl_ast::Import;
use kmpl_check::{Module, ModuleImport};

use crate::diagnostics::{self, Sources};
use crate::manifest::{self, Manifest};
use crate::remote::{self, Fetch, Remote};

/// The `std:` libraries, by name.
const LIBRARIES: [(&str, &str); 8] = [
    ("bits", include_str!("../std/bits/mod.kmpl")),
    ("fs", include_str!("../std/fs/mod.kmpl")),
    ("io", include_str!("../std/io/mod.kmpl")),
    ("linalg", include_str!("../std/linalg/mod.kmpl")),
    ("random", include_str!("../std/random/mod.kmpl")),
    ("stats", include_str!("../std/stats/mod.kmpl")),
    ("text", include_str!("../std/text/mod.kmpl")),
    ("time", include_str!("../std/time/mod.kmpl")),
];

#[derive(Debug, Clone, Copy, Default)]
pub struct LoadOptions {
    /// Nothing may be added to `kmpl.lock`.
    pub locked: bool,
}

/// Errors are already rendered for printing.
pub fn load(
    path: &str,
    sources: &mut Sources,
    options: LoadOptions,
) -> Result<Vec<Module>, String> {
    load_with(
        path,
        sources,
        options,
        remote::default_cache(),
        &remote::http_get,
    )
}

fn load_with(
    path: &str,
    sources: &mut Sources,
    options: LoadOptions,
    cache: Option<PathBuf>,
    fetch: Fetch<'_>,
) -> Result<Vec<Module>, String> {
    let file = Path::new(path);
    let text = std::fs::read_to_string(file).map_err(|e| format!("{path}: {e}"))?;
    let root = file.parent().unwrap_or(Path::new("")).to_path_buf();

    let mut loader = Loader {
        root: normalize(&root),
        sources,
        stack: Vec::new(),
        loaded: HashSet::new(),
        modules: Vec::new(),
        manifest: None,
        options,
        cache,
        fetch,
        remote: None,
        first_is_generated: false,
    };
    let first = Found {
        id: loader.module_id(file),
        path: path.to_owned(),
        text,
        dir: Some(root),
        link: None,
    };
    loader.module(first, true)?;
    if let Some(remote) = &loader.remote {
        remote.save()?;
    }
    Ok(loader.modules)
}

/// Finds the dependencies `names` of `project` and everything they import,
/// recording in `remote` what was downloaded. It goes through a small
/// generated file that imports them, so they are found exactly as a program
/// would find them. The caller decides whether to save the result.
pub fn fetch_dependencies<'f>(
    project: Manifest,
    names: &[String],
    remote: Remote<'f>,
    fetch: Fetch<'f>,
) -> Result<Remote<'f>, String> {
    let mut sources = Sources::default();
    let mut loader = Loader {
        root: normalize(&project.dir),
        sources: &mut sources,
        stack: Vec::new(),
        loaded: HashSet::new(),
        modules: Vec::new(),
        manifest: Some(Some(project)),
        options: LoadOptions::default(),
        cache: None,
        fetch,
        remote: Some(remote),
        first_is_generated: true,
    };
    let text: String = names
        .iter()
        .enumerate()
        .map(|(index, name)| format!("import d{index} from \"{name}\";\n"))
        .collect();
    let first = Found {
        id: "kmpl".to_owned(),
        path: "kmpl".to_owned(),
        text,
        dir: None,
        link: None,
    };
    loader.module(first, true)?;
    Ok(loader.remote.expect("given above"))
}

/// A module that was located, and not necessarily loaded yet.
struct Found {
    /// The full name of the module, like `a.b`.
    id: String,
    /// Shown in error messages.
    path: String,
    text: String,
    /// Where its relative imports start from, for a local file.
    dir: Option<PathBuf>,
    /// The link it was downloaded from.
    link: Option<String>,
}

struct Loader<'s, 'f> {
    /// Directory of the first file.
    root: PathBuf,
    sources: &'s mut Sources,
    /// The modules being loaded, to notice a cycle.
    stack: Vec<String>,
    loaded: HashSet<String>,
    modules: Vec<Module>,
    /// Read the first time a plain name is imported.
    manifest: Option<Option<Manifest>>,
    options: LoadOptions,
    cache: Option<PathBuf>,
    fetch: Fetch<'f>,
    /// Opened the first time a link is imported.
    remote: Option<Remote<'f>>,
    /// The first file has no text worth showing in an error.
    first_is_generated: bool,
}

impl<'f> Loader<'_, 'f> {
    fn module(&mut self, module: Found, is_first: bool) -> Result<(), String> {
        let program = self.parse(&module.path, &module.text)?;

        self.stack.push(module.id.clone());
        let mut imports = Vec::new();
        for import in &program.imports {
            let target = self.find(import, &module)?;
            let target_id = target.id.clone();

            if let Some(start) = self.stack.iter().position(|m| *m == target_id) {
                let chain = self.stack[start..].join(" -> ");
                return Err(self.sources.render(
                    &format!("import cycle: {chain} -> {target_id}"),
                    import.span,
                ));
            }
            if !self.loaded.contains(&target_id) {
                self.module(target, false)?;
            }

            imports.push(ModuleImport {
                names: import.names.clone(),
                target: target_id,
            });
        }
        self.stack.pop();
        self.loaded.insert(module.id.clone());

        self.modules.push(Module {
            prefix: if is_first { String::new() } else { module.id },
            imports,
            program,
        });
        Ok(())
    }

    /// Works out which module an import refers to and reads it.
    fn find(&mut self, import: &Import, from: &Found) -> Result<Found, String> {
        let source = import.source.as_str();
        if let Some(name) = source.strip_prefix("std:") {
            return self.find_library(name, import);
        }
        if remote::is_link(source) {
            return self.find_link(source, import);
        }
        if source.starts_with("./") || source.starts_with("../") {
            if let Some(link) = &from.link {
                let url = remote::join(link, source);
                return self.download(url, import);
            }
            let Some(dir) = &from.dir else {
                return Err(self.error("this module cannot import relative paths", import));
            };
            return self.find_local(&dir.join(source), import);
        }
        if source.contains("://") {
            return Err(self.error("only `https://` links can be imported", import));
        }
        if from.link.is_some() {
            return Err(self.error(
                &format!("a downloaded module cannot use the name `{source}` from `kmpl.toml`"),
                import,
            ));
        }
        if source.starts_with('/') || source.contains('/') {
            return Err(self.error(
                &format!("`{source}` is not a module path, start a relative path with `./`"),
                import,
            ));
        }
        self.find_dependency(import)
    }

    /// A plain name: what `kmpl.toml` says it is.
    fn find_dependency(&mut self, import: &Import) -> Result<Found, String> {
        let name = &import.source;
        if self.manifest.is_none() {
            let found = manifest::find(&self.root).map_err(|e| self.error(&e, import))?;
            self.manifest = Some(found);
        }
        let Some(Some(project)) = &self.manifest else {
            return Err(self.error(
                &format!("`{name}` needs a `{}` that lists it", manifest::FILE_NAME),
                import,
            ));
        };
        let Some(value) = project.dependencies.get(name) else {
            return Err(self.error(
                &format!(
                    "`{name}` is not listed in `{}` under `[dependencies]`",
                    project.dir.join(manifest::FILE_NAME).display()
                ),
                import,
            ));
        };
        if remote::is_link(value) {
            let link = value.clone();
            return self.find_link(&link, import);
        }
        let path = project.dir.join(value);
        self.find_local(&path, import)
    }

    /// The object that knows `kmpl.lock`. It lives next to `kmpl.toml`, or
    /// next to the program if there is none.
    fn remote(&mut self, import: &Import) -> Result<&mut Remote<'f>, String> {
        if self.remote.is_none() {
            if self.manifest.is_none() {
                let found = manifest::find(&self.root).map_err(|e| self.error(&e, import))?;
                self.manifest = Some(found);
            }
            let project = self
                .manifest
                .iter()
                .flatten()
                .next()
                .map(|m| m.dir.as_path());
            let path = remote::lock_path(project, &self.root);
            let opened = Remote::open(path, self.options.locked, self.cache.clone(), self.fetch)
                .map_err(|e| self.error(&e, import))?;
            self.remote = Some(opened);
        }
        Ok(self.remote.as_mut().expect("just opened"))
    }

    fn find_link(&mut self, link: &str, import: &Import) -> Result<Found, String> {
        let url = self.remote(import)?.file_url(link);
        let url = url.map_err(|e| self.error(&e, import))?;
        self.download(url, import)
    }

    fn download(&mut self, url: String, import: &Import) -> Result<Found, String> {
        let text = self.remote(import)?.read(&url);
        let text = text.map_err(|e| self.error(&e, import))?;
        Ok(Found {
            id: format!("remote:{url}"),
            path: url.clone(),
            text,
            dir: None,
            link: Some(url),
        })
    }

    fn find_library(&self, name: &str, import: &Import) -> Result<Found, String> {
        let Some((_, text)) = LIBRARIES.iter().find(|(library, _)| *library == name) else {
            let known: Vec<&str> = LIBRARIES.iter().map(|(library, _)| *library).collect();
            return Err(self.error(
                &format!(
                    "there is no library `std:{name}`, there are: {}",
                    known.join(", ")
                ),
                import,
            ));
        };
        Ok(Found {
            id: format!("std:{name}"),
            path: import.source.clone(),
            text: (*text).to_owned(),
            dir: None,
            link: None,
        })
    }

    fn error(&self, message: &str, import: &Import) -> String {
        if self.first_is_generated && self.stack.len() == 1 {
            return message.to_owned();
        }
        self.sources.render(message, import.source_span)
    }

    /// `path` is the file itself if it ends in `.kmpl`. Otherwise it is
    /// `path.kmpl` or `path/mod.kmpl`, and there must be exactly one.
    fn find_local(&self, path: &Path, import: &Import) -> Result<Found, String> {
        let source = &import.source;
        let path = normalize(path);
        let candidates = if path.extension().is_some_and(|ext| ext == "kmpl") {
            vec![path.clone()]
        } else {
            let mut with_extension = OsString::from(path.as_os_str());
            with_extension.push(".kmpl");
            vec![PathBuf::from(with_extension), path.join("mod.kmpl")]
        };

        let shown = |file: &PathBuf| format!("`{}`", file.display());
        let found: Vec<&PathBuf> = candidates.iter().filter(|c| c.is_file()).collect();
        let file = match found[..] {
            [file] => file,
            [] => {
                let tried: Vec<String> = candidates.iter().map(shown).collect();
                return Err(self.error(
                    &format!("cannot find module `{source}`: tried {}", tried.join(", ")),
                    import,
                ));
            }
            _ => {
                let both: Vec<String> = candidates.iter().map(shown).collect();
                return Err(self.error(
                    &format!("module `{source}` is ambiguous: {}", both.join(" and ")),
                    import,
                ));
            }
        };

        let text = std::fs::read_to_string(file).map_err(|e| {
            self.error(
                &format!(
                    "cannot read module `{source}` from `{}`: {e}",
                    file.display()
                ),
                import,
            )
        })?;
        Ok(Found {
            id: self.module_id(file),
            path: file.to_string_lossy().into_owned(),
            text,
            dir: Some(file.parent().unwrap_or(Path::new("")).to_path_buf()),
            link: None,
        })
    }

    /// The path from the directory of the first file, joined by dots. A
    /// file outside that directory starts with `up`.
    fn module_id(&self, file: &Path) -> String {
        let file = normalize(file);
        let (relative, outside) = match file.strip_prefix(&self.root) {
            Ok(relative) => (relative.to_path_buf(), false),
            Err(_) => (file.clone(), true),
        };

        let mut parts = Vec::new();
        if outside {
            parts.push("up".to_owned());
        }
        for component in relative.with_extension("").components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().replace('.', "_")),
                Component::ParentDir => parts.push("up".to_owned()),
                _ => {}
            }
        }
        parts.join(".")
    }

    fn parse(&mut self, path: &str, text: &str) -> Result<kmpl_ast::Program, String> {
        let mut tokens = kmpl_lexer::lex(text).map_err(|span| {
            let message = diagnostics::lex_error_message(text, span);
            diagnostics::render(path, text, &message, span)
        })?;

        let start = self.sources.add(path, text);
        for (_, span) in &mut tokens {
            span.start += start;
            span.end += start;
        }

        kmpl_parse::parse(tokens).map_err(|err| self.sources.render(&err.message, err.span))
    }
}

/// Removes `.` and resolves `..` by looking only at the text of the path.
fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(result.components().next_back(), Some(Component::Normal(_))) =>
            {
                result.pop();
            }
            other => result.push(other),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const COMMIT: &str = "4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a";

    fn packet(text: &str) -> String {
        format!("{:04x}{text}", text.len() + 4)
    }

    #[test]
    fn a_link_in_kmpl_toml_and_the_relative_imports_behind_it() {
        let dir = std::env::temp_dir().join(format!("kmpl-loader-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("kmpl.toml"),
            "version = 1\n[dependencies]\npb = \"https://github.com/u/pb/v1\"\n",
        )
        .unwrap();
        let main = dir.join("src/main.kmpl");
        std::fs::write(
            &main,
            "import pb from \"pb\";\nfn main() { print(pb.N); }\n",
        )
        .unwrap();

        let raw = format!("https://raw.githubusercontent.com/u/pb/{COMMIT}");
        let refs = packet(&format!("{COMMIT} refs/tags/v1\0caps\n")) + "0000";
        let files: HashMap<String, String> = [
            (
                "https://github.com/u/pb.git/info/refs?service=git-upload-pack".to_owned(),
                refs,
            ),
            (
                format!("{raw}/mod.kmpl"),
                "import wire from \"./wire.kmpl\";\nconst N: int = wire.M;\n".to_owned(),
            ),
            (format!("{raw}/wire.kmpl"), "const M: int = 3;\n".to_owned()),
        ]
        .into();
        let fetch = |url: &str| Ok(files.get(url).map(|text| text.clone().into_bytes()));

        let mut sources = Sources::default();
        let path = main.to_string_lossy().into_owned();
        let modules = load_with(&path, &mut sources, LoadOptions::default(), None, &fetch).unwrap();
        assert_eq!(modules.len(), 3);

        let lock = std::fs::read_to_string(dir.join("kmpl.lock")).unwrap();
        assert!(
            lock.contains(&format!("https://github.com/u/pb/v1 commit:{COMMIT}")),
            "{lock}"
        );
        assert!(lock.contains(&format!("{raw}/mod.kmpl sha256:")), "{lock}");
        assert!(lock.contains(&format!("{raw}/wire.kmpl sha256:")), "{lock}");

        // Everything is recorded now, so `--locked` is satisfied.
        let mut sources = Sources::default();
        let locked = LoadOptions { locked: true };
        load_with(&path, &mut sources, locked, None, &fetch).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
