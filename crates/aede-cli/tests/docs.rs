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

#[test]
fn the_front_page_names_every_page_of_the_manual() {
    // A page nobody links to is a page nobody reads, and splitting a document
    // is exactly the moment one gets orphaned: it survives the split, keeps
    // its content, and quietly leaves the manual.
    let root = root();
    let readme = std::fs::read_to_string(root.join("README.md")).expect("a README");
    let named: BTreeSet<String> = links(&readme)
        .into_iter()
        .map(|l| l.split('#').next().unwrap_or_default().to_string())
        .collect();

    let mut pages = Vec::new();
    markdown_files(&root.join("docs"), &mut pages);
    let mut orphans: Vec<String> = Vec::new();
    for page in pages {
        let relative = page
            .strip_prefix(&root)
            .unwrap_or(&page)
            .display()
            .to_string();
        if !named.contains(&relative) {
            orphans.push(relative);
        }
    }
    orphans.sort();
    assert!(
        orphans.is_empty(),
        "pages the README does not name:\n  {}",
        orphans.join("\n  ")
    );
}
