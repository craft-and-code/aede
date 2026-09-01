//! The documentation is checked like code, because it is read like code.
//!
//! The manual was one file until it reached a hundred kilobytes, and splitting
//! it into `docs/` turned every cross-reference from an anchor inside one
//! document into a path between files. Anchors fail loudly — the browser goes
//! nowhere — but paths fail *silently*: a renamed page leaves links that look
//! fine in the source and 404 on the web. A dead link is a promise the project
//! made and did not keep, and nothing else in this repository is allowed to
//! break without a test failing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// Every Markdown file of the repository, ignoring what is not ours.
fn markdown_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        match path.is_dir() {
            true => markdown_files(&path, found),
            false if name.ends_with(".md") => found.push(path),
            false => {}
        }
    }
}

/// The anchor GitHub gives a heading: lowercase, punctuation dropped, spaces
/// turned into hyphens.
fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.trim().chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.extend(c.to_lowercase());
        } else if c.is_whitespace() {
            out.push('-');
        }
    }
    out
}

/// Every anchor a file offers, which is one per heading.
///
/// Headings inside fenced code blocks are not headings — a shell prompt with a
/// comment starting in `#` would otherwise announce an anchor nobody can reach.
fn anchors(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            let title = rest.trim_start_matches('#').trim();
            if !title.is_empty() && rest.starts_with([' ', '#']) {
                found.insert(slug(title));
            }
        }
    }
    found
}

/// Every link target a file carries, as it is written.
///
/// Hand-parsed rather than by regular expression: this crate has one
/// dependency and it is not a regex engine.
fn links(text: &str) -> Vec<String> {
    let bytes: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == ']' && bytes[i + 1] == '(' {
            let mut j = i + 2;
            let mut target = String::new();
            while j < bytes.len() && bytes[j] != ')' && bytes[j] != '\n' {
                target.push(bytes[j]);
                j += 1;
            }
            if j < bytes.len() && bytes[j] == ')' {
                out.push(target);
                i = j;
            }
        }
        i += 1;
    }
    out
}

#[test]
fn every_link_in_the_documentation_leads_somewhere() {
    let root = root();
    let mut files = Vec::new();
    markdown_files(&root, &mut files);
    files.sort();
    assert!(
        files.len() > 15,
        "the manual is a folder of pages now: {} found",
        files.len()
    );

    let mut broken: Vec<String> = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a readable page");
        let here = file.parent().expect("a folder");
        let shown = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .display()
            .to_string();

        for link in links(&text) {
            let link = link.trim();
            if link.starts_with("http://") || link.starts_with("https://") || link.is_empty() {
                continue;
            }
            let (path, anchor) = match link.split_once('#') {
                Some((path, anchor)) => (path, Some(anchor)),
                None => (link, None),
            };

            // Where the anchor has to exist: this page, or the one named.
            let target = match path.is_empty() {
                true => file.clone(),
                false => here.join(path),
            };
            if !target.exists() {
                broken.push(format!("{shown}: \"{link}\" leads to no such file"));
                continue;
            }
            let Some(anchor) = anchor.filter(|_| target.extension().is_some_and(|e| e == "md"))
            else {
                continue;
            };
            let text = std::fs::read_to_string(&target).unwrap_or_default();
            if !anchors(&text).contains(anchor) {
                broken.push(format!("{shown}: \"{link}\" names no heading of that page"));
            }
        }
    }
    assert!(broken.is_empty(), "dead links:\n  {}", broken.join("\n  "));
}

/// The file a path denotes, symlinks and `..` resolved away.
///
/// Two spellings of one file compare equal here and nowhere else: this is what
/// lets a link written `docs/library.md` be recognised as the page the walk
/// found, on a system whose own spelling of it is `docs\library.md`.
fn resolved(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[test]
fn the_front_page_names_every_page_of_the_manual() {
    // A page nobody links to is a page nobody reads, and splitting a document
    // is exactly the moment one gets orphaned: it survives the split, keeps
    // its content, and quietly leaves the manual.
    //
    // The comparison is between *files*, not between the texts that name them.
    // It used to render each page's path and look that string up among the
    // README's links, which held on Unix by luck: `Path::display` spells a
    // separator the way the platform does, so on Windows every page of the
    // manual read as an orphan — `docs\library.md` is not the string
    // `docs/library.md`, though both name the same file. A link is a path, and
    // paths are compared as paths.
    //
    // What guards that is the Windows leg of CI, and only it: the defect is
    // conditional on the platform, and it cannot be staged on another one.
    // An attempt to reproduce it here through `..` instead of a separator was
    // written and removed — `PathBuf::join` folds `..` away on Windows and not
    // on Unix, so the reproduction had a platform in it too, and asserting how
    // a path *renders* is the very habit that caused this.
    let root = root();
    let readme = std::fs::read_to_string(root.join("README.md")).expect("a README");
    let named: BTreeSet<PathBuf> = links(&readme)
        .into_iter()
        .map(|l| l.split('#').next().unwrap_or_default().trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with("http://") && !l.starts_with("https://"))
        .map(|l| resolved(&root.join(l)))
        .collect();

    // A check whose two sides are both empty passes and proves nothing, and
    // resolving is exactly what could empty one of them — a link that resolves
    // nowhere is dropped silently by `canonicalize`. Both sides are therefore
    // required to hold something first.
    assert!(
        named.len() > 15,
        "the README names {} local files: the links stopped being read",
        named.len()
    );

    let mut pages = Vec::new();
    markdown_files(&root.join("docs"), &mut pages);
    assert!(
        pages.len() > 15,
        "the manual is a folder of pages: {} found",
        pages.len()
    );

    let mut orphans: Vec<String> = Vec::new();
    for page in pages {
        if !named.contains(&resolved(&page)) {
            orphans.push(
                page.strip_prefix(&root)
                    .unwrap_or(&page)
                    .display()
                    .to_string(),
            );
        }
    }
    orphans.sort();
    assert!(
        orphans.is_empty(),
        "pages the README does not name:\n  {}",
        orphans.join("\n  ")
    );
}
