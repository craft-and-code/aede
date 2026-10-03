#!/usr/bin/env python3
"""Validate generated site links, fragments, locale counterparts and metadata."""
from __future__ import annotations

import argparse
import json
import posixpath
import re
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


class Page(HTMLParser):
    def __init__(self, source):
        super().__init__(convert_charrefs=True)
        self.ids, self.duplicates, self.links, self.meta, self.alternates = set(), [], [], {}, []
        self.canonical, self.title, self.language, self.in_title = None, "", None, False
        self.feed(source)

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        if attrs.get("id"):
            if attrs["id"] in self.ids: self.duplicates.append(attrs["id"])
            self.ids.add(attrs["id"])
        if tag == "html": self.language = attrs.get("lang")
        if tag == "title": self.in_title = True
        if tag == "meta": self.meta[attrs.get("name") or attrs.get("property")] = attrs.get("content", "")
        if tag == "link" and attrs.get("rel") == "canonical": self.canonical = attrs.get("href")
        if tag == "link" and attrs.get("rel") == "alternate" and attrs.get("hreflang"): self.alternates.append((attrs["hreflang"], attrs.get("href", "")))
        for attribute in ("href", "src", "poster"):
            if attrs.get(attribute): self.links.append((tag, attrs[attribute]))

    def handle_endtag(self, tag):
        if tag == "title": self.in_title = False

    def handle_data(self, text):
        if self.in_title: self.title += text


def check(directory: Path, require_rustdoc=False) -> list[str]:
    directory = directory.resolve()
    failures = []
    pages = {}
    for path in directory.rglob("*.html"):
        filename = path.relative_to(directory).as_posix()
        if filename.startswith("docs/rust/"): continue
        source = path.read_text(encoding="utf-8")
        forbidden = re.search(r"[\x00-\x08\x0b\x0c\x0e-\x1f]", source)
        if forbidden: failures.append(f"{filename}: invisible control character U+{ord(forbidden[0]):04X}")
        pages[filename] = Page(source)
    if not pages: return ["No generated HTML pages found; build the site first."]
    canonical_urls = {}
    for filename, page in pages.items():
        if not page.title.strip(): failures.append(f"{filename}: missing title")
        if not page.meta.get("description"): failures.append(f"{filename}: missing description")
        if page.language not in ("fr", "en"): failures.append(f"{filename}: unsupported or missing document language")
        if not page.canonical or not page.canonical.startswith("https://"): failures.append(f"{filename}: missing HTTPS canonical URL")
        elif page.canonical in canonical_urls: failures.append(f"{filename}: canonical also used by {canonical_urls[page.canonical]}")
        else: canonical_urls[page.canonical] = filename
        if page.duplicates: failures.append(f"{filename}: duplicate IDs {', '.join(page.duplicates)}")
        if filename != "docs/index.html" and not {"fr", "en"} <= {language for language, _ in page.alternates}: failures.append(f"{filename}: missing bilingual hreflang counterparts")
        for tag, href in page.links:
            parsed = urlsplit(href)
            if parsed.scheme or href.startswith("//"): continue
            if parsed.path.startswith("/"):
                # The production project lives under /aede; absolute domain
                # paths would silently escape it during Pages publication.
                failures.append(f"{filename}: non-portable absolute local link {href}"); continue
            target_name = posixpath.normpath(posixpath.join(posixpath.dirname(filename), unquote(parsed.path))) if parsed.path else filename
            if target_name.startswith("../"): failures.append(f"{filename}: link escapes site {href}"); continue
            target = directory / target_name
            if parsed.path.endswith("/") or target.is_dir(): target_name = posixpath.join(target_name, "index.html"); target = directory / target_name
            if target_name == "docs/rust/aede_core/index.html" and not require_rustdoc and not target.exists(): continue
            if not target.is_file(): failures.append(f"{filename}: missing local target {href}"); continue
            if parsed.fragment and target.suffix == ".html":
                target_page = pages.get(target_name)
                if target_page is None:
                    target_page = Page(target.read_text(encoding="utf-8"))
                fragment = unquote(parsed.fragment)
                if fragment not in target_page.ids: failures.append(f"{filename}: missing fragment {href}")
    # Verify counterpart canonical URLs point to pages in this build instead
    # of only looking plausible in the HTML head.
    for filename, page in pages.items():
        for language, url in page.alternates:
            if language in ("fr", "en") and url not in canonical_urls:
                failures.append(f"{filename}: hreflang {language} has no generated counterpart {url}")
    rustdoc = directory / "docs/rust/aede_core/index.html"
    if require_rustdoc and not rustdoc.exists(): failures.append("Missing generated RustDoc at docs/rust/aede_core/index.html")
    manifest = directory / "site-pages.json"
    if manifest.exists():
        for page in json.loads(manifest.read_text(encoding="utf-8"))["pages"]:
            for language in ("fr", "en"):
                expected = f"docs/{language}/{page['slug']}.html"
                if expected not in pages: failures.append(f"Manifest page not published: {expected}")
    return sorted(set(failures))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path, default=Path(__file__).resolve().parent.parent / "dist-site")
    parser.add_argument("--require-rustdoc", action="store_true")
    args = parser.parse_args()
    failures = check(args.directory, args.require_rustdoc)
    if failures:
        print("\n".join(failures)); raise SystemExit(1)
    print("Site metadata, local links, fragments and locale counterparts are valid.")


if __name__ == "__main__": main()
