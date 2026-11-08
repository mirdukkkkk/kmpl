//! Reads the file given on the command line and everything it imports.
//!
//! `import a.b;` reads `a/b.kmpl` next to the importing file. A module is
//! known by its path from the directory of the first file (`lib.a.b`), so a
//! file imported from several places is read once. Modules are listed after
//! the ones they import.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use kmpl_check::{Module, ModuleImport};

use crate::diagnostics::{self, Sources};

/// Errors are already rendered for printing.
pub fn load(path: &str, sources: &mut Sources) -> Result<Vec<Module>, String> {
    let file = Path::new(path);
    let text = std::fs::read_to_string(file).map_err(|e| format!("{path}: {e}"))?;
    let stem = file
        .file_stem()
        .map_or(String::new(), |s| s.to_string_lossy().into_owned());

    let mut loader = Loader {
        dir: file.parent().unwrap_or(Path::new("")).to_path_buf(),
        sources,
        stack: Vec::new(),
        loaded: HashSet::new(),
        modules: Vec::new(),
    };
    loader.module(&[stem], true, path, &text)?;
    Ok(loader.modules)
}

struct Loader<'s> {
    /// Directory of the first file.
    dir: PathBuf,
    sources: &'s mut Sources,
    /// The modules being loaded, to notice a cycle.
    stack: Vec<String>,
    loaded: HashSet<String>,
    modules: Vec<Module>,
}

impl Loader<'_> {
    fn module(
        &mut self,
        parts: &[String],
        is_first: bool,
        path: &str,
        text: &str,
    ) -> Result<(), String> {
        let name = parts.join(".");
        let program = self.parse(path, text)?;

        self.stack.push(name.clone());
        let mut imports = Vec::new();
        for import in &program.imports {
            // Relative to this file, which is in the directory `parts` names.
            let mut target_parts = parts[..parts.len() - 1].to_vec();
            target_parts.extend(import.path.iter().map(|part| part.name.clone()));
            let target = target_parts.join(".");

            if let Some(start) = self.stack.iter().position(|m| *m == target) {
                let chain = self.stack[start..].join(" -> ");
                return Err(self
                    .sources
                    .render(&format!("import cycle: {chain} -> {target}"), import.span));
            }

            if !self.loaded.contains(&target) {
                let file = self.dir.join(format!("{}.kmpl", target_parts.join("/")));
                let file = file.to_string_lossy().into_owned();
                let text = std::fs::read_to_string(&file).map_err(|e| {
                    self.sources.render(
                        &format!("cannot read module `{target}` from `{file}`: {e}"),
                        import.span,
                    )
                })?;
                self.module(&target_parts, false, &file, &text)?;
            }

            imports.push(ModuleImport {
                name: import.path.last().expect("at least one part").clone(),
                target,
            });
        }
        self.stack.pop();
        self.loaded.insert(name.clone());

        self.modules.push(Module {
            prefix: if is_first { String::new() } else { name },
            imports,
            program,
        });
        Ok(())
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
