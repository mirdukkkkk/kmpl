//! Modules that come from a link, and `kmpl.lock`.
//!
//! `https://github.com/user/repo/v1.2.0` names a tag, a branch or a commit.
//! The first time it is seen it is turned into an exact commit, and from
//! then on `kmpl.lock` keeps that commit, so a moved tag or a new push to a
//! branch changes nothing until the lock file says so. Every downloaded
//! file is recorded there with its hash and checked against it.
//!
//! ```text
//! https://github.com/user/repo/main commit:4f2a...
//! https://raw.githubusercontent.com/user/repo/4f2a.../mod.kmpl sha256:9c1e...
//! ```
//!
//! Files are cached by their hash, so a project that is locked needs no
//! network once they have been downloaded.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Downloads a link. `None` means it does not exist.
pub type Fetch<'f> = &'f dyn Fn(&str) -> Result<Option<Vec<u8>>, String>;

pub const LOCK_FILE: &str = "kmpl.lock";

/// The real network.
pub fn http_get(url: &str) -> Result<Option<Vec<u8>>, String> {
    let response = minreq::get(url)
        .with_timeout(60)
        .send()
        .map_err(|e| format!("cannot download `{url}`: {e}"))?;
    match response.status_code {
        200 => Ok(Some(response.into_bytes())),
        404 => Ok(None),
        code => Err(format!("cannot download `{url}`: HTTP {code}")),
    }
}

/// `$XDG_CACHE_HOME/kmpl`, or `~/.cache/kmpl`.
pub fn default_cache() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".cache"),
    };
    Some(base.join("kmpl"))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Default, PartialEq)]
struct Lock {
    /// A GitHub link -> the commit it means.
    commits: BTreeMap<String, String>,
    /// The link of a file -> the hash of its contents.
    hashes: BTreeMap<String, String>,
}

impl Lock {
    fn parse(text: &str) -> Result<Lock, String> {
        let mut lock = Lock::default();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry = line.split_once(' ').and_then(|(url, value)| {
                value
                    .split_once(':')
                    .map(|(kind, value)| (url, kind, value))
            });
            let number = index + 1;
            match entry {
                Some((url, "commit", value)) => {
                    lock.commits.insert(url.to_owned(), value.to_owned());
                }
                Some((url, "sha256", value)) => {
                    lock.hashes.insert(url.to_owned(), value.to_owned());
                }
                _ => return Err(format!("{LOCK_FILE}:{number}: cannot understand this line")),
            }
        }
        Ok(lock)
    }

    fn text(&self) -> String {
        let mut text = String::new();
        for (url, commit) in &self.commits {
            text.push_str(&format!("{url} commit:{commit}\n"));
        }
        for (url, hash) in &self.hashes {
            text.push_str(&format!("{url} sha256:{hash}\n"));
        }
        text
    }
}

/// Is this a link to a module?
pub fn is_link(source: &str) -> bool {
    source.starts_with("https://")
}

pub struct Remote<'f> {
    fetch: Fetch<'f>,
    lock: Lock,
    lock_path: PathBuf,
    changed: bool,
    /// Nothing may be added to the lock file.
    locked: bool,
    cache: Option<PathBuf>,
}

impl<'f> Remote<'f> {
    pub fn open(
        lock_path: PathBuf,
        locked: bool,
        cache: Option<PathBuf>,
        fetch: Fetch<'f>,
    ) -> Result<Remote<'f>, String> {
        let lock = match std::fs::read_to_string(&lock_path) {
            Ok(text) => Lock::parse(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Lock::default(),
            Err(e) => return Err(format!("cannot read `{}`: {e}", lock_path.display())),
        };
        Ok(Remote {
            fetch,
            lock,
            lock_path,
            changed: false,
            locked,
            cache,
        })
    }

    /// Writes `kmpl.lock` if something new was recorded.
    pub fn save(&self) -> Result<(), String> {
        if !self.changed {
            return Ok(());
        }
        std::fs::write(&self.lock_path, self.lock.text())
            .map_err(|e| format!("cannot write `{}`: {e}", self.lock_path.display()))
    }

    fn require_unlocked(&self, what: &str) -> Result<(), String> {
        match self.locked {
            true => Err(format!(
                "{what} is not in `{LOCK_FILE}`, and `--locked` was given"
            )),
            false => Ok(()),
        }
    }

    /// The link of the file that `link`, from an import or `kmpl.toml`, means.
    pub fn file_url(&mut self, link: &str) -> Result<String, String> {
        match github_parts(link)? {
            Some(parts) => {
                let commit = self.commit(link, &parts)?;
                let path = with_default_file(parts.path);
                Ok(format!(
                    "https://raw.githubusercontent.com/{}/{}/{commit}/{path}",
                    parts.owner, parts.repo
                ))
            }
            None if link.ends_with(".kmpl") => Ok(link.to_owned()),
            None => Ok(format!("{}/mod.kmpl", link.trim_end_matches('/'))),
        }
    }

    /// The commit a GitHub link is locked to, finding it if it is new.
    fn commit(&mut self, link: &str, parts: &GitHubLink) -> Result<String, String> {
        // Everything under one tag or branch shares one commit.
        let key = format!(
            "https://github.com/{}/{}/{}",
            parts.owner, parts.repo, parts.reference
        );
        if let Some(commit) = self.lock.commits.get(&key) {
            return Ok(commit.clone());
        }

        self.require_unlocked(&format!("`{key}`"))?;
        let commit = if is_commit(parts.reference) {
            parts.reference.to_owned()
        } else {
            self.find_commit(link, parts)?
        };
        self.lock.commits.insert(key, commit.clone());
        self.changed = true;
        Ok(commit)
    }

    /// Asks GitHub what a tag or branch points to, the way `git` does.
    fn find_commit(&self, link: &str, parts: &GitHubLink) -> Result<String, String> {
        let url = format!(
            "https://github.com/{}/{}.git/info/refs?service=git-upload-pack",
            parts.owner, parts.repo
        );
        let Some(body) = (self.fetch)(&url)? else {
            return Err(format!("the repository of `{link}` does not exist"));
        };
        commit_of(&body, parts.reference)
            .ok_or_else(|| format!("no tag or branch `{}` in `{link}`", parts.reference))
    }

    /// The text of a file, checked against `kmpl.lock`.
    pub fn read(&mut self, url: &str) -> Result<String, String> {
        let expected = self.lock.hashes.get(url).cloned();

        if let Some(hash) = &expected
            && let Some(bytes) = self.cached(hash)
        {
            return text_of(url, bytes);
        }
        if expected.is_none() {
            self.require_unlocked(&format!("`{url}`"))?;
        }

        let Some(bytes) = (self.fetch)(url)? else {
            return Err(format!("`{url}` does not exist"));
        };
        let actual = sha256(&bytes);
        match expected {
            Some(hash) if hash != actual => {
                return Err(format!(
                    "`{url}` changed: `{LOCK_FILE}` expects sha256:{hash}, it is sha256:{actual}"
                ));
            }
            Some(_) => {}
            None => {
                self.lock.hashes.insert(url.to_owned(), actual.clone());
                self.changed = true;
            }
        }
        self.store(&actual, &bytes);
        text_of(url, bytes)
    }

    /// A file from the cache, only if it really has this hash.
    fn cached(&self, hash: &str) -> Option<Vec<u8>> {
        let bytes = std::fs::read(self.cache.as_ref()?.join(hash)).ok()?;
        (sha256(&bytes) == hash).then_some(bytes)
    }

    /// Without a cache the file is downloaded again next time, nothing worse.
    fn store(&self, hash: &str, bytes: &[u8]) {
        if let Some(cache) = &self.cache {
            let _ = std::fs::create_dir_all(cache);
            let _ = std::fs::write(cache.join(hash), bytes);
        }
    }
}

fn text_of(url: &str, bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("`{url}` is not valid UTF-8"))
}

#[derive(Debug, PartialEq)]
struct GitHubLink<'a> {
    owner: &'a str,
    repo: &'a str,
    reference: &'a str,
    /// Inside the repository, possibly empty.
    path: &'a str,
}

/// `https://github.com/owner/repo/reference[/path]`. `None` for a link to
/// anything else, including a file attached to a release.
fn github_parts(link: &str) -> Result<Option<GitHubLink<'_>>, String> {
    let Some(rest) = link.strip_prefix("https://github.com/") else {
        return Ok(None);
    };
    let mut segments = rest.trim_end_matches('/').splitn(4, '/');
    let (Some(owner), Some(repo)) = (segments.next(), segments.next()) else {
        return Err(format!(
            "`{link}` needs a tag, branch or commit after the repository"
        ));
    };
    let Some(reference) = segments.next() else {
        return Err(format!(
            "`{link}` needs a tag, branch or commit after the repository"
        ));
    };
    match reference {
        "releases" | "raw" => Ok(None),
        "blob" | "tree" => Err(format!(
            "`{link}` is a web page, write `https://github.com/{owner}/{repo}/<tag or branch>`"
        )),
        _ => Ok(Some(GitHubLink {
            owner,
            repo,
            reference,
            path: segments.next().unwrap_or(""),
        })),
    }
}

/// A file named exactly, or the `mod.kmpl` of a directory.
fn with_default_file(path: &str) -> String {
    if path.ends_with(".kmpl") {
        path.to_owned()
    } else if path.is_empty() {
        "mod.kmpl".to_owned()
    } else {
        format!("{}/mod.kmpl", path.trim_end_matches('/'))
    }
}

fn is_commit(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Finds a tag or branch in the reply of `info/refs`. A tag wins over a
/// branch of the same name, and an annotated tag means the commit under it.
fn commit_of(body: &[u8], reference: &str) -> Option<String> {
    let mut refs = BTreeMap::new();
    let mut rest = body;
    while rest.len() >= 4 {
        let length = usize::from_str_radix(std::str::from_utf8(&rest[..4]).ok()?, 16).ok()?;
        // Zero marks the end of a section and has no contents.
        if length == 0 {
            rest = &rest[4..];
            continue;
        }
        let line = std::str::from_utf8(rest.get(4..length)?).ok()?;
        rest = &rest[length..];

        // Capabilities follow the first reference, after a zero byte.
        let line = line.split('\0').next()?.trim_end();
        if let Some((commit, name)) = line.split_once(' ') {
            refs.insert(name.to_owned(), commit.to_owned());
        }
    }

    [
        format!("refs/tags/{reference}^{{}}"),
        format!("refs/tags/{reference}"),
        format!("refs/heads/{reference}"),
    ]
    .iter()
    .find_map(|name| refs.get(name).cloned())
}

/// A relative import inside the file at `base`.
pub fn join(base: &str, relative: &str) -> String {
    let (origin, path) = match base
        .strip_prefix("https://")
        .and_then(|r| r.split_once('/'))
    {
        Some((host, path)) => (format!("https://{host}"), path),
        None => (base.to_owned(), ""),
    };

    let mut segments: Vec<&str> = path.split('/').collect();
    segments.pop();
    for part in relative.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }

    let joined = segments.join("/");
    format!("{origin}/{}", with_default_file(&joined))
}

/// Where `kmpl.lock` lives: next to `kmpl.toml`, or else next to the program.
pub fn lock_path(project: Option<&Path>, program_dir: &Path) -> PathBuf {
    project.unwrap_or(program_dir).join(LOCK_FILE)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    const COMMIT: &str = "4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a4f2a";
    const NEWER: &str = "1111111111111111111111111111111111111111";

    fn packet(text: &str) -> String {
        format!("{:04x}{text}", text.len() + 4)
    }

    fn refs_reply(lines: &[String]) -> Vec<u8> {
        let mut body = packet("# service=git-upload-pack\n") + "0000";
        for (index, line) in lines.iter().enumerate() {
            let line = match index {
                0 => format!("{line}\0multi_ack thin-pack\n"),
                _ => format!("{line}\n"),
            };
            body.push_str(&packet(&line));
        }
        body.push_str("0000");
        body.into_bytes()
    }

    #[test]
    fn finds_tags_branches_and_the_commit_under_an_annotated_tag() {
        let body = refs_reply(&[
            format!("{NEWER} HEAD"),
            format!("{NEWER} refs/heads/main"),
            format!("{NEWER} refs/heads/v1"),
            format!("{NEWER} refs/tags/v1"),
            format!("{COMMIT} refs/tags/v1^{{}}"),
            format!("{COMMIT} refs/tags/plain"),
        ]);
        assert_eq!(commit_of(&body, "main").as_deref(), Some(NEWER));
        assert_eq!(commit_of(&body, "v1").as_deref(), Some(COMMIT));
        assert_eq!(commit_of(&body, "plain").as_deref(), Some(COMMIT));
        assert_eq!(commit_of(&body, "nope"), None);
    }

    #[test]
    fn github_links_are_taken_apart() {
        let parts = github_parts("https://github.com/u/r/v1.2.0")
            .unwrap()
            .unwrap();
        assert_eq!(
            (parts.owner, parts.repo, parts.reference, parts.path),
            ("u", "r", "v1.2.0", "")
        );
        let parts = github_parts("https://github.com/u/r/main/lib/a.kmpl")
            .unwrap()
            .unwrap();
        assert_eq!((parts.reference, parts.path), ("main", "lib/a.kmpl"));

        assert_eq!(github_parts("https://example.org/x").unwrap(), None);
        assert_eq!(
            github_parts("https://github.com/u/r/releases/download/v1/a.kmpl").unwrap(),
            None
        );
        assert!(github_parts("https://github.com/u/r").is_err());
        assert!(github_parts("https://github.com/u/r/blob/main/a.kmpl").is_err());
    }

    #[test]
    fn relative_imports_follow_the_link() {
        let base = "https://h.org/a/b/mod.kmpl";
        assert_eq!(join(base, "./x.kmpl"), "https://h.org/a/b/x.kmpl");
        assert_eq!(join(base, "./x"), "https://h.org/a/b/x/mod.kmpl");
        assert_eq!(join(base, "../y.kmpl"), "https://h.org/a/y.kmpl");
    }

    #[test]
    fn the_lock_file_survives_a_round_trip() {
        let text = format!("https://a commit:{COMMIT}\nhttps://b sha256:abc\n");
        assert_eq!(Lock::parse(&text).unwrap().text(), text);
        assert!(Lock::parse("nonsense").unwrap_err().contains(":1:"));
    }

    /// A fake network with a counter of requests.
    struct Server {
        files: RefCell<HashMap<String, Vec<u8>>>,
        requests: RefCell<usize>,
    }

    impl Server {
        fn new(files: &[(&str, &[u8])]) -> Server {
            Server {
                files: RefCell::new(
                    files
                        .iter()
                        .map(|(url, bytes)| ((*url).to_owned(), bytes.to_vec()))
                        .collect(),
                ),
                requests: RefCell::new(0),
            }
        }

        fn get(&self, url: &str) -> Result<Option<Vec<u8>>, String> {
            *self.requests.borrow_mut() += 1;
            Ok(self.files.borrow().get(url).cloned())
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kmpl-remote-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const REFS: &str = "https://github.com/u/r.git/info/refs?service=git-upload-pack";

    fn raw(commit: &str) -> String {
        format!("https://raw.githubusercontent.com/u/r/{commit}/mod.kmpl")
    }

    #[test]
    fn a_branch_stays_on_its_commit_until_the_lock_changes() {
        let dir = temp("pin");
        let reply = refs_reply(&[format!("{COMMIT} refs/heads/main")]);
        let server = Server::new(&[(REFS, &reply), (&raw(COMMIT), b"const N: int = 1;")]);
        let fetch = |url: &str| server.get(url);
        let lock = dir.join(LOCK_FILE);
        let cache = Some(dir.join("cache"));
        let link = "https://github.com/u/r/main";

        let mut remote = Remote::open(lock.clone(), false, cache.clone(), &fetch).unwrap();
        let url = remote.file_url(link).unwrap();
        assert_eq!(url, raw(COMMIT));
        assert_eq!(remote.read(&url).unwrap(), "const N: int = 1;");
        remote.save().unwrap();
        let text = std::fs::read_to_string(&lock).unwrap();
        assert!(text.contains(&format!("{link} commit:{COMMIT}")), "{text}");
        assert!(text.contains(&format!("{} sha256:", raw(COMMIT))), "{text}");

        // The branch moves on. Nothing is asked, and the commit is the same.
        let moved = refs_reply(&[format!("{NEWER} refs/heads/main")]);
        server.files.borrow_mut().insert(REFS.to_owned(), moved);
        *server.requests.borrow_mut() = 0;
        let mut remote = Remote::open(lock, true, cache, &fetch).unwrap();
        let url = remote.file_url(link).unwrap();
        assert_eq!(url, raw(COMMIT));
        assert_eq!(remote.read(&url).unwrap(), "const N: int = 1;");
        assert_eq!(*server.requests.borrow(), 0);
    }

    #[test]
    fn a_changed_file_is_refused() {
        let dir = temp("changed");
        let url = "https://h.org/a.kmpl";
        let server = Server::new(&[(url, b"one")]);
        let fetch = |u: &str| server.get(u);
        let lock = dir.join(LOCK_FILE);

        let mut remote = Remote::open(lock.clone(), false, None, &fetch).unwrap();
        remote.read(url).unwrap();
        remote.save().unwrap();

        server
            .files
            .borrow_mut()
            .insert(url.to_owned(), b"two".to_vec());
        let mut remote = Remote::open(lock, false, None, &fetch).unwrap();
        let error = remote.read(url).unwrap_err();
        assert!(error.contains("changed"), "{error}");
    }

    #[test]
    fn a_damaged_cache_is_not_trusted() {
        let dir = temp("damaged");
        let url = "https://h.org/a.kmpl";
        let server = Server::new(&[(url, b"good")]);
        let fetch = |u: &str| server.get(u);
        let lock = dir.join(LOCK_FILE);
        let cache = dir.join("cache");

        let mut remote = Remote::open(lock.clone(), false, Some(cache.clone()), &fetch).unwrap();
        remote.read(url).unwrap();
        remote.save().unwrap();
        std::fs::write(cache.join(sha256(b"good")), b"evil").unwrap();

        let mut remote = Remote::open(lock, false, Some(cache), &fetch).unwrap();
        assert_eq!(remote.read(url).unwrap(), "good");
    }

    #[test]
    fn locked_refuses_anything_new() {
        let dir = temp("locked");
        let url = "https://h.org/a.kmpl";
        let server = Server::new(&[(url, b"x")]);
        let fetch = |u: &str| server.get(u);
        let mut remote = Remote::open(dir.join(LOCK_FILE), true, None, &fetch).unwrap();
        assert!(remote.read(url).unwrap_err().contains("--locked"));
        assert!(
            remote
                .file_url("https://github.com/u/r/main")
                .unwrap_err()
                .contains("--locked")
        );
        assert_eq!(*server.requests.borrow(), 0);
    }

    #[test]
    fn a_missing_tag_is_reported() {
        let dir = temp("no_tag");
        let reply = refs_reply(&[format!("{COMMIT} refs/heads/main")]);
        let server = Server::new(&[(REFS, &reply)]);
        let fetch = |u: &str| server.get(u);
        let mut remote = Remote::open(dir.join(LOCK_FILE), false, None, &fetch).unwrap();
        let error = remote.file_url("https://github.com/u/r/v9").unwrap_err();
        assert!(error.contains("no tag or branch `v9`"), "{error}");
    }

    #[test]
    fn a_commit_in_the_link_needs_no_question() {
        let dir = temp("commit");
        let server = Server::new(&[]);
        let fetch = |u: &str| server.get(u);
        let mut remote = Remote::open(dir.join(LOCK_FILE), false, None, &fetch).unwrap();
        let url = remote
            .file_url(&format!("https://github.com/u/r/{COMMIT}"))
            .unwrap();
        assert_eq!(url, raw(COMMIT));
        assert_eq!(*server.requests.borrow(), 0);
    }
}
