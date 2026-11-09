//! Reads the file given on the command line and everything it imports.
//!
//! `import x from "./a/b";` reads `a/b.kmpl` or `a/b/mod.kmpl` next to the
//! importing file. A module is known by its path from the directory of the
//! first file (`a.b`), so a file imported from several places is read once.
//! Modules are listed after the ones they import.
//!
//! `"std:fs"` is a library that is part of `kmpl` itself. A plain name like
//! `"pb"` is looked up in `kmpl.toml`.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use kmpl_ast::Import;
use kmpl_check::{Module, ModuleImport};

use crate::diagnostics::{self, Sources};
use crate::manifest::{self, Manifest};

/// The `std:` libraries, by name.
const LIBRARIES: [(&str, &str); 4] = [
    ("bits", include_str!("../std/bits/mod.kmpl")),
    ("fs", include_str!("../std/fs/mod.kmpl")),
    ("io", include_str!("../std/io/mod.kmpl")),
    ("text", include_str!("../std/text/mod.kmpl")),
];

/// Errors are already rendered for printing.
pub fn load(path: &str, sources: &mut Sources) -> Result<Vec<Module>, String> {
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
    };
    let first = Found {
        id: loader.module_id(file),
        path: path.to_owned(),
        text,
        dir: Some(root),
    };
    loader.module(first, true)?;
    Ok(loader.modules)
}

/// A module that was located, and not necessarily loaded yet.
struct Found {
    /// The full name of the module, like `a.b`.
    id: String,
    /// Shown in error messages.
    path: String,
    text: String,
    /// Where its relative imports start from.
    dir: Option<PathBuf>,
}

struct Loader<'s> {
    /// Directory of the first file.
    root: PathBuf,
    sources: &'s mut Sources,
    /// The modules being loaded, to notice a cycle.
    stack: Vec<String>,
    loaded: HashSet<String>,
    modules: Vec<Module>,
    /// Read the first time a plain name is imported.
    manifest: Option<Option<Manifest>>,
}

impl Loader<'_> {
    fn module(&mut self, module: Found, is_first: bool) -> Result<(), String> {
        let program = self.parse(&module.path, &module.text)?;

        self.stack.push(module.id.clone());
        let mut imports = Vec::new();
        for import in &program.imports {
            let target = self.find(import, module.dir.as_deref())?;
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
    fn find(&mut self, import: &Import, dir: Option<&Path>) -> Result<Found, String> {
        let source = import.source.as_str();
        if let Some(name) = source.strip_prefix("std:") {
            return self.find_library(name, import);
        }
        if source.starts_with("./") || source.starts_with("../") {
            let Some(dir) = dir else {
                return Err(self.error("this module cannot import relative paths", import));
            };
            return self.find_local(&dir.join(source), import);
        }
        if source.contains("://") {
            return Err(self.error("importing from a link is not supported yet", import));
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
        if value.contains("://") {
            return Err(self.error("importing from a link is not supported yet", import));
        }
        let path = project.dir.join(value);
        self.find_local(&path, import)
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
        })
    }

    fn error(&self, message: &str, import: &Import) -> String {
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
