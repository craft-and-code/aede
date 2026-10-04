#!/usr/bin/env python3
"""Publish repository-authored Markdown as a dependency-free bilingual website."""

from __future__ import annotations

import argparse
from functools import lru_cache
import hashlib
import html
import importlib.util
import json
import posixpath
import re
import shutil
import sys
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "https://github.com/craft-and-code/aede"
REPOSITORY_BRANCH = "master"
BASE_URL = "https://craft-and-code.github.io/aede/"
TOPICS = {
    "library": ("manual", "Understanding your library", "Comprendre sa bibliothèque"),
    "querying": ("cli", "Query language", "Langage de requête"),
    "browsing": ("cli", "Browsing the catalog", "Parcourir le catalogue"),
    "commands": ("cli", "Command overview", "Vue d’ensemble des commandes"),
    "copying": ("cli", "Copying and conversion", "Copie et conversion"),
    "playlists": ("cli", "Playlists", "Listes de lecture"),
    "annotating": ("cli", "Favourites, ratings and notes", "Favoris, notes et annotations"),
    "formats": ("manual", "Audio formats", "Formats audio"),
    "integrity": ("cli", "File integrity", "Intégrité des fichiers"),
    "spectrograms": ("cli", "Spectrograms", "Spectrogrammes"),
    "imported-analyses": ("dsp", "FlacCompagnon analysis reports", "Rapports d’analyse FlacCompagnon"),
    "sources": ("cli", "External sources and provenance", "Sources externes et provenance"),
    "licensing": ("manual", "Licensing", "Licences"),
    "api": ("server", "API contract", "Contrat de l’API"),
    "operating": ("server", "Operating reference", "Référence d’exploitation"),
}
GROUPS = {
    "fr": [("manual", "Manuel utilisateur"), ("cli", "CLI — Commandes"), ("server", "Serveur — API"), ("dsp", "DSP — Audio"), ("compatibility", "Compatible Aède")],
    "en": [("manual", "User manual"), ("cli", "CLI — Commands"), ("server", "Server — API"), ("dsp", "DSP — Audio"), ("compatibility", "Compatible Aède")],
}
PROJECT_STATS_MARKER = "<!-- project-statistics -->"
COMMAND_GROUPS = {
    "scan": "catalog", "stats": "catalog", "doctor": "catalog", "roots": "catalog",
    "albums": "browse", "album": "browse", "artists": "browse", "artist": "browse", "track": "browse", "tracks": "browse", "genres": "browse", "genre": "browse", "labels": "browse", "label": "browse", "countries": "browse", "years": "browse", "roles": "browse", "works": "browse", "work": "browse", "release-groups": "browse", "search": "browse", "query": "browse",
    "fetch": "enrich", "fingerprint": "enrich", "identify": "enrich", "review": "enrich", "merge": "enrich", "credit": "enrich", "credits": "enrich", "relation": "enrich",
    "love": "personal", "rating": "personal", "tag": "personal", "note": "personal", "plays": "personal", "played": "personal", "collection": "personal", "collections": "personal",
    "copy": "files", "check": "files", "analyze": "files", "spectrogram": "files", "playlist": "files", "export": "files", "backup": "files", "restore": "files",
    "play": "playback", "serve": "playback", "cancel": "playback", "help": "playback",
}
COMMAND_LABELS = {
    "fr": {"catalog": "Construire et vérifier", "browse": "Explorer et chercher", "enrich": "Enrichir et corriger", "personal": "Personnaliser", "files": "Préserver et exporter", "playback": "Écouter et administrer", "navigation": "Parcourir le catalogue", "selection": "Chercher et sélectionner", "enrichment": "Enrichir et corriger", "maintenance": "Préserver et administrer", "other": "Autres commandes", "reference": "Références thématiques"},
    "en": {"catalog": "Build and inspect", "browse": "Browse and search", "enrich": "Enrich and correct", "personal": "Personalise", "files": "Preserve and export", "playback": "Listen and administer", "navigation": "Browse the catalog", "selection": "Find and select", "enrichment": "Enrich and correct", "maintenance": "Preserve and administer", "other": "Other commands", "reference": "Topic references"},
}


def esc(value: object) -> str:
    return html.escape(str(value), quote=True)


def slugify(value: str) -> str:
    """GitHub-compatible fragment base, including accented letters."""
    plain = html.unescape(re.sub(r"<[^>]*>", "", value)).lower()
    plain = re.sub(r"[^\w\-\s]", "", plain, flags=re.UNICODE)
    return re.sub(r"\s", "-", plain).strip("-") or "section"


@dataclass
class Rendered:
    html: str
    headings: list[tuple[int, str, str]]
    title_id: str


class Markdown:
    """Small CommonMark/GFM subset for trusted, versioned documentation.

    Supports ATX/setext headings, fenced/indented code, paragraphs, blockquotes,
    nested ordered/unordered lists, task lists, GFM tables, inline code, emphasis,
    strikethrough, links/images (also reference-style), autolinks, and authored
    raw HTML. This is not a sanitizer and is never used on visitor input.
    """

    def __init__(self, rewrite=lambda href: href):
        self.rewrite = rewrite
        self.references = {}
        self.headings = []
        self.used_ids = {}
        self.title_id = "article-title"
        self.inline_counter = 0

    def inline(self, value: str) -> str:
        tokens = []
        self.inline_counter += 1
        token_prefix = str(self.inline_counter) + ":"

        def protect(rendered: str) -> str:
            tokens.append(rendered)
            return f"\x00{token_prefix}{len(tokens) - 1}\x00"

        # Code is isolated before any markup or escaping; query operators and
        # angle-bracket placeholders must remain literal in both code forms.
        value = re.sub(r"(`+)(.+?)\1", lambda m: protect("<code>" + esc(m[2].replace("\n", " ")) + "</code>"), value, flags=re.S)
        value = re.sub(r"\\([\\`*_{}\[\]()#+.!<>|~-])", lambda m: protect(esc(m[1])), value)

        def link(match):
            image, label, destination = match.groups()
            destination = destination.strip()
            title = ""
            titled = re.fullmatch(r'(.*?)(?:\s+["\'](.*)["\'])', destination, re.S)
            if titled:
                destination, title = titled.groups()
            if destination.startswith("<") and destination.endswith(">"):
                destination = destination[1:-1]
            href = self.rewrite(destination)
            attributes = f' title="{esc(title)}"' if title else ""
            if image:
                return protect(f'<img src="{esc(href)}" alt="{esc(label)}" loading="lazy"{attributes}>')
            return protect(f'<a href="{esc(href)}"{attributes}>{self.inline(label)}</a>')

        # Parentheses in link destinations are allowed to one nested level.
        value = re.sub(r"(!?)\[([^\]]+)\]\(((?:[^()]|\([^()]*\))*)\)", link, value)

        def reference(match):
            image, label, key = match.groups()
            reference = self.references.get((key or label).strip().lower())
            if reference is None:
                return match[0]
            destination, title = reference
            return link(type("Match", (), {"groups": lambda _self: (image, label, destination + (f' "{title}"' if title else ""))})())

        value = re.sub(r"(!?)\[([^\]]+)\]\[([^\]]*)\]", reference, value)
        value = re.sub(r"<((?:https?://|mailto:)[^>]+)>", lambda m: protect(f'<a href="{esc(m[1])}">{esc(m[1].removeprefix("mailto:"))}</a>'), value)
        # Inline raw HTML is repository-authored. URLs are rewritten in block
        # HTML separately as well, and no untrusted HTML reaches this renderer.
        value = re.sub(r"</?[A-Za-z][^>]*>", lambda m: protect(self.raw_html(m[0])), value)
        value = esc(value)
        value = re.sub(r"\*\*(.+?)\*\*|__(.+?)__", lambda m: "<strong>" + (m[1] or m[2]) + "</strong>", value)
        value = re.sub(r"(?<!\w)\*([^*]+)\*|(?<!\w)_([^_]+)_(?!\w)", lambda m: "<em>" + (m[1] or m[2]) + "</em>", value)
        value = re.sub(r"~~(.+?)~~", r"<del>\1</del>", value)
        value = re.sub(r" {2,}\n", "<br>\n", value)
        pattern = r"\x00" + re.escape(token_prefix) + r"(\d+)\x00"
        while re.search(pattern, value):
            value = re.sub(pattern, lambda m: tokens[int(m[1])], value)
        return value

    def raw_html(self, value: str) -> str:
        return re.sub(r'(href|src)="([^"]+)"', lambda m: m[1] + '="' + esc(self.rewrite(html.unescape(m[2]))) + '"', value)

    @staticmethod
    def cells(line: str) -> list[str]:
        # Pipes inside a code span are data, not table separators.
        cells, part, code_ticks, index = [], [], 0, 0
        value = line.strip().strip("|")
        while index < len(value):
            char = value[index]
            if char == "\\" and index + 1 < len(value):
                part.extend(value[index:index + 2]); index += 2; continue
            if char == "`":
                end = index
                while end < len(value) and value[end] == "`": end += 1
                count = end - index
                code_ticks = 0 if code_ticks == count else count if not code_ticks else code_ticks
                part.extend(value[index:end]); index = end; continue
            if char == "|" and not code_ticks:
                cells.append("".join(part).strip()); part = []
            else:
                part.append(char)
            index += 1
        cells.append("".join(part).strip())
        return cells

    def heading(self, depth: int, title: str, remove_title: bool) -> str:
        content = self.inline(title.strip())
        base = slugify(content)
        count = self.used_ids.get(base, 0)
        self.used_ids[base] = count + 1
        identifier = base if count == 0 else f"{base}-{count}"
        plain = html.unescape(re.sub(r"<[^>]*>", "", content))
        if depth == 1 and remove_title:
            self.title_id = identifier
            return ""
        self.headings.append((depth, identifier, plain))
        return f'<h{depth} id="{esc(identifier)}">{content}<a class="heading-link" href="#{esc(identifier)}" aria-label="{esc(plain)}">#</a></h{depth}>'

    def blocks(self, lines: list[str], remove_title=False) -> str:
        output, index = [], 0
        marker = re.compile(r"^( *)([-+*]|\d+[.)])\s+(.*)$")
        while index < len(lines):
            line = lines[index]
            if not line.strip(): index += 1; continue
            fence = re.match(r"^\s{0,3}(`{3,}|~{3,})([^ ]*)\s*$", line)
            if fence:
                delimiter, language = fence.groups(); index += 1; body = []
                while index < len(lines) and not re.match(r"^\s{0,3}" + re.escape(delimiter[0]) + "{" + str(len(delimiter)) + r",}\s*$", lines[index]):
                    body.append(lines[index]); index += 1
                index += 1
                output.append(f'<pre><code class="language-{esc(language)}">{esc(chr(10).join(body))}\n</code></pre>'); continue
            heading = re.match(r"^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$", line)
            if heading:
                output.append(self.heading(len(heading[1]), heading[2], remove_title)); index += 1; continue
            if index + 1 < len(lines) and re.fullmatch(r"\s*(=+|-+)\s*", lines[index + 1]) and line.strip():
                output.append(self.heading(1 if "=" in lines[index + 1] else 2, line, remove_title)); index += 2; continue
            if re.fullmatch(r"\s{0,3}(?:\*\s*){3,}|\s{0,3}(?:-\s*){3,}|\s{0,3}(?:_\s*){3,}", line):
                output.append("<hr>"); index += 1; continue
            if line.lstrip().startswith(">"):
                body = []
                while index < len(lines) and lines[index].lstrip().startswith(">"):
                    body.append(re.sub(r"^\s*> ?", "", lines[index])); index += 1
                output.append("<blockquote>" + self.blocks(body) + "</blockquote>"); continue
            if index + 1 < len(lines) and "|" in line and all(re.fullmatch(r":?-{3,}:?", cell.replace(" ", "")) for cell in self.cells(lines[index + 1])):
                header = self.cells(line); alignments = self.cells(lines[index + 1]); index += 2
                rows = []
                while index < len(lines) and "|" in lines[index] and lines[index].strip():
                    rows.append(self.cells(lines[index])); index += 1
                def cell(value, column, tag):
                    align = alignments[column] if column < len(alignments) else ""
                    style = ' class="align-center"' if align.startswith(":") and align.endswith(":") else ' class="align-right"' if align.endswith(":") else ""
                    return f"<{tag}{style}>" + self.inline(value) + f"</{tag}>"
                output.append('<div class="guide-table" tabindex="0" role="region" aria-label="Table"><table><thead><tr>' + "".join(cell(v, n, "th") for n, v in enumerate(header)) + "</tr></thead><tbody>" + "".join("<tr>" + "".join(cell(row[n] if n < len(row) else "", n, "td") for n in range(len(header))) + "</tr>" for row in rows) + "</tbody></table></div>"); continue
            item = marker.match(line)
            if item:
                base_indent = len(item[1]); ordered = item[2][0].isdigit(); list_items = []; start = int(re.match(r"\d+", item[2])[0]) if ordered else 1
                while index < len(lines):
                    item = marker.match(lines[index])
                    if not item or len(item[1]) != base_indent or item[2][0].isdigit() != ordered: break
                    content_indent = len(item[1]) + len(item[2]) + 1
                    body = [item[3]]; index += 1
                    while index < len(lines):
                        current = lines[index]
                        next_item = marker.match(current)
                        if next_item and len(next_item[1]) <= base_indent: break
                        if current.strip() and len(current) - len(current.lstrip()) <= base_indent: break
                        if not current.strip():
                            if index + 1 < len(lines) and lines[index + 1].strip() and len(lines[index + 1]) - len(lines[index + 1].lstrip()) <= base_indent: break
                            body.append(""); index += 1; continue
                        body.append(current[min(content_indent, len(current) - len(current.lstrip())):]); index += 1
                    task = re.match(r"^\[([ xX])\] (.*)$", body[0])
                    prefix = ""
                    if task:
                        body[0] = task[2]
                        prefix = '<input type="checkbox" disabled' + (' checked' if task[1].lower() == "x" else '') + '> '
                    rendered = self.blocks(body)
                    if len(body) == 1 and rendered.startswith("<p>"):
                        rendered = rendered[3:-4]
                    list_items.append("<li>" + prefix + rendered + "</li>")
                tag = "ol" if ordered else "ul"; attribute = f' start="{start}"' if ordered and start != 1 else ""
                output.append(f"<{tag}{attribute}>" + "".join(list_items) + f"</{tag}>"); continue
            if line.startswith("    "):
                body = []
                while index < len(lines) and (lines[index].startswith("    ") or not lines[index].strip()):
                    body.append(lines[index][4:] if lines[index].startswith("    ") else ""); index += 1
                output.append("<pre><code>" + esc("\n".join(body).rstrip()) + "\n</code></pre>"); continue
            raw = re.match(r"^\s*</?(details|summary|div|figure|figcaption|table|thead|tbody|tr|td|th|section|aside|picture|video|iframe|script|style|!--)\b", line)
            if raw:
                # details/summary are line blocks so Markdown between their
                # tags continues to render, as it does in our authored guides.
                output.append(self.raw_html(line)); index += 1; continue
            paragraph = [line]; index += 1
            while index < len(lines) and lines[index].strip():
                candidate = lines[index]
                if re.match(r"^\s{0,3}(#{1,6}\s|`{3,}|~{3,}|>|<details|</details|<summary|</summary)", candidate) or marker.match(candidate) or re.fullmatch(r"\s*[-*_]{3,}\s*", candidate): break
                if index + 1 < len(lines) and "|" in candidate and re.fullmatch(r"[\s|:\-]+", lines[index + 1]): break
                paragraph.append(candidate); index += 1
            output.append("<p>" + self.inline("\n".join(paragraph)) + "</p>")
        return "\n".join(output)

    def render(self, source: str, remove_title=True) -> Rendered:
        forbidden = re.search(r"[\x00-\x08\x0b\x0c\x0e-\x1f]", source)
        if forbidden:
            raise ValueError(f"Forbidden invisible control character U+{ord(forbidden[0]):04X} in Markdown source")
        lines = []
        active_fence = None
        for line in source.replace("\r\n", "\n").expandtabs(4).split("\n"):
            fence = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
            if fence:
                if active_fence is None: active_fence = fence[1]
                elif fence[1][0] == active_fence[0] and len(fence[1]) >= len(active_fence): active_fence = None
                lines.append(line)
                continue
            reference = re.match(r'^\s{0,3}\[([^\]]+)\]:\s*<?([^\s>]+)>?(?:\s+["\'](.*)["\'])?\s*$', line) if active_fence is None else None
            if reference:
                self.references[reference[1].lower()] = (reference[2], reference[3] or "")
            else:
                lines.append(line)
        content = self.blocks(lines, remove_title)
        return Rendered(content, self.headings, self.title_id)


def load_pages(root: Path) -> list[dict]:
    pages = []
    manifests = [root / "docs/site-manual-cli.json", root / "docs/site-server-dsp.json"]
    manifests.extend(sorted(path for path in (root / "docs").glob("site-*.json") if path not in manifests))
    for manifest in manifests:
        if manifest.exists():
            data = json.loads(manifest.read_text(encoding="utf-8"))
            pages.extend(data if isinstance(data, list) else data["pages"])
    registered = {source for page in pages for source in page.get("source", {}).values()}
    for name, (section, english, french) in TOPICS.items():
        source = f"docs/{name}.md"
        if source in registered or not (root / source).exists(): continue
        translated = f"docs/fr/{name}.md"
        pages.append({"slug": f"{section}/reference/{name}", "section": section, "reference": True, "title": {"en": english, "fr": french}, "description": {"en": f"Detailed Aède reference: {english.lower()}.", "fr": f"Référence détaillée d’Aède : {french.lower()}."}, "source": {"en": source, "fr": translated if (root / translated).exists() else source}})
    seen = set()
    for page in pages:
        if page["slug"] in seen: raise ValueError(f"Duplicate page slug: {page['slug']}")
        if page["section"] not in {group[0] for group in GROUPS["en"]}: raise ValueError(f"Unknown section: {page['section']}")
        if page.get("generated") not in (None, "project-statistics"): raise ValueError(f"Unknown generated page content: {page['generated']}")
        if not re.fullmatch(r"[a-z0-9][a-z0-9/_-]*", page["slug"]) or ".." in page["slug"]: raise ValueError(f"Unsafe page slug: {page['slug']}")
        seen.add(page["slug"])
        for language in ("en", "fr"):
            if not (root / page["source"][language]).is_file(): raise ValueError(f"Missing {language} source: {page['source'][language]}")
    return pages


def relative(source: str, destination: str) -> str:
    return posixpath.relpath(destination, posixpath.dirname(source) or ".")


def asset_href(output: str, filename: str, versions=None) -> str:
    href = relative(output, filename)
    version = (versions or {}).get(filename)
    return href + (f"?v={version}" if version else "")


@lru_cache(maxsize=512)
def heading_outline(path: str, modified: int, size: int):
    rendered = Markdown().render(Path(path).read_text(encoding="utf-8"))
    return rendered.title_id, tuple(rendered.headings)


def translated_fragment(fragment: str, page: dict, language: str, root: Path) -> str:
    if not fragment: return ""
    identifier = unquote(fragment)
    if not all((root / page["source"][locale]).is_file() for locale in ("en", "fr")): return fragment
    def outline(locale):
        path = root / page["source"][locale]
        facts = path.stat()
        return heading_outline(str(path), facts.st_mtime_ns, facts.st_size)
    desired_title, desired = outline(language)
    if identifier in {desired_title, *(entry[1] for entry in desired)}: return fragment
    other = "en" if language == "fr" else "fr"
    original_title, original = outline(other)
    mapping = {original_title: desired_title}
    if len(original) == len(desired):
        for before, after in zip(original, desired):
            if before[0] == after[0]: mapping[before[1]] = after[1]
    return quote(mapping.get(identifier, identifier), safe="-_.~")


def rewrite_link(href: str, source: str, output: str, language: str, pages: list[dict], root=ROOT) -> str:
    parsed = urlsplit(href)
    if parsed.scheme or href.startswith(("#", "//", "/")): return href
    file = unquote(parsed.path)
    if not file: return href
    target = posixpath.normpath(posixpath.join(posixpath.dirname(source), file))
    mapping = {}
    for page in pages:
        for value in page["source"].values(): mapping.setdefault(value, page["slug"])
    if target == "crates/aede-server/README.md":
        route_page = next((p for p in pages if p["section"] == "server" and (p["slug"].endswith("http") or p["slug"].endswith("routes") or p["slug"].endswith("reference"))), None)
        if route_page:
            # The beginner route reference has its own sections rather than
            # pretending old README fragments have the same heading names.
            return relative(output, f"docs/{language}/{route_page['slug']}.html")
    if target == "site/index.html":
        return relative(output, "index.html" if language == "fr" else "en/index.html")
    if target in mapping:
        destination = f"docs/{language}/{mapping[target]}.html"
        page = next(page for page in pages if page["slug"] == mapping[target])
        fragment = translated_fragment(parsed.fragment, page, language, root)
        return relative(output, destination) + ("?" + parsed.query if parsed.query else "") + ("#" + fragment if fragment else "")
    if file.endswith(".md") or target.startswith(("docs/design/", "docs/coding/", "crates/")) or target in ("README.md", "CLAUDE.md", "LICENSE", "schema.sql"):
        return REPOSITORY + f"/blob/{REPOSITORY_BRANCH}/" + quote(target, safe="/") + ("#" + parsed.fragment if parsed.fragment else "")
    if (root / target).is_file():
        return relative(output, "source-assets/" + target) + ("#" + parsed.fragment if parsed.fragment else "")
    return href


def sidebar(pages: list[dict], language: str, output: str, current="") -> str:
    sections = []
    for section, title in GROUPS[language]:
        relevant = [p for p in pages if p["section"] == section and p.get("navigation", True)]
        if not relevant: continue
        buckets = {}
        for page in relevant:
            group = "reference" if page.get("reference") else page.get("group", COMMAND_GROUPS.get(page.get("command"), "other")) if section == "cli" and page.get("command") else ""
            buckets.setdefault(group, []).append(page)
        groups = []
        for group, entries in buckets.items():
            links = []
            for page in entries:
                active = ' aria-current="page"' if page["slug"] == current else ""
                command = f'<code>{esc(page["command"])}</code> ' if page.get("command") else ""
                href = relative(output, "docs/" + language + "/" + page["slug"] + ".html")
                label = page["title"][language]
                if page.get("command"):
                    label = re.sub(r"^" + re.escape(page["command"]) + r"\s*[—–-]\s*", "", label, flags=re.I)
                links.append(f'<li data-nav-item><a href="{esc(href)}"{active}>{command}{esc(label)}</a></li>')
            label = f'<p class="guide-nav-subtitle">{COMMAND_LABELS[language].get(group, esc(group))}</p>' if group else ""
            groups.append(f'<div class="guide-nav-bucket" data-nav-bucket>{label}<ul>{"".join(links)}</ul></div>')
        open_attribute = " open" if not current or any(p["slug"] == current for p in relevant) or section == "manual" else ""
        sections.append(f'<details data-nav-section="{section}"{open_attribute}><summary>{title}<span aria-hidden="true">⌄</span></summary>{"".join(groups)}</details>')
    placeholder = "Chercher une commande ou un sujet" if language == "fr" else "Find a command or topic"
    home = relative(output, f"docs/{language}/index.html")
    rust = relative(output, "docs/rust/aede_core/index.html")
    return f'<a class="guide-docs-home" href="{esc(home)}">{"Toute la documentation" if language == "fr" else "All documentation"}</a><label class="guide-search">{placeholder}<input type="search" placeholder="{placeholder}" aria-controls="guide-nav"></label><p class="guide-search-status" role="status"></p><nav id="guide-nav" aria-label="Documentation">{"".join(sections)}<a class="guide-rustdoc" href="{esc(rust)}">RustDoc ↗</a></nav>'


def document_shell(page: dict, language: str, content: Rendered, pages: list[dict], base_url=BASE_URL, counterpart_content=None, asset_versions=None) -> str:
    slug = page["slug"]
    output = f"docs/{language}/{slug}.html" if slug else f"docs/{language}/index.html"
    other = "en" if language == "fr" else "fr"
    fragment_map = {content.title_id: counterpart_content.title_id} if counterpart_content else {}
    if counterpart_content and len(content.headings) == len(counterpart_content.headings):
        for current, translated in zip(content.headings, counterpart_content.headings):
            if current[0] == translated[0]: fragment_map[current[1]] = translated[1]
    counterpart = f"docs/{other}/{slug}.html" if slug else f"docs/{other}/index.html"
    title, description = page["title"][language], page["description"][language]
    canonical = base_url + output
    home = relative(output, "index.html" if language == "fr" else "en/index.html")
    source_path = page.get("source", {}).get(language)
    fallback = source_path and language == "fr" and source_path == page["source"].get("en")
    notice = '<aside class="translation-notice">Cette référence est actuellement publiée en anglais. Les guides principaux sont disponibles en français.</aside>' if fallback else ""
    section = next((label for key, label in GROUPS[language] if key == page.get("section")), "Documentation")
    toc_links = "".join(f'<li class="toc-level-{level}"><a href="#{esc(identifier)}">{esc(label)}</a></li>' for level, identifier, label in content.headings if level in (2, 3))
    source_link = f'<a href="{REPOSITORY}/edit/{REPOSITORY_BRANCH}/{quote(source_path, safe="/")}">{"Modifier la source Markdown" if language == "fr" else "Edit the Markdown source"}</a>' if source_path else ""
    source_note = "Le Markdown du dépôt est la source unique de cette page." if language == "fr" else "Repository Markdown is this page’s single source."
    if page.get("generated") == "project-statistics":
        source_note = "Le texte vient du Markdown du dépôt ; les statistiques sont calculées depuis les sources et l’inventaire fourni." if language == "fr" else "The text comes from repository Markdown; statistics are calculated from sources and the supplied inventory."
    explainer = f'<section class="dsp-explainer" data-explainer="{esc(page["explainer"])}" aria-label="{"Illustration interactive du DSP" if language == "fr" else "Interactive DSP illustration"}"></section>' if page.get("explainer") else ""
    body_lang = ' lang="en"' if fallback else ""
    privacy_page = next((candidate for candidate in pages if candidate["slug"] == "manual/privacy"), None)
    privacy_footer = f'<a href="{esc(relative(output, "docs/" + language + "/manual/privacy.html"))}">{"Confidentialité" if language == "fr" else "Privacy"}</a>' if privacy_page else ""
    prev_next = ""
    if slug:
        index = next(i for i, candidate in enumerate(pages) if candidate["slug"] == slug)
        siblings = [p for p in pages if p["section"] == page["section"] and not p.get("reference") and p.get("navigation", True)]
        if page in siblings:
            item_index = siblings.index(page)
            previous = siblings[item_index - 1] if item_index else None
            following = siblings[item_index + 1] if item_index + 1 < len(siblings) else None
            def neighbour(candidate, label):
                if candidate is None: return "<span></span>"
                href = relative(output, "docs/" + language + "/" + candidate["slug"] + ".html")
                return f'<a href="{esc(href)}"><small>{label}</small><span class="guide-next-title">{esc(candidate["title"][language])}</span></a>'
            prev_next = '<nav class="guide-next" aria-label="' + ("Pages voisines" if language == "fr" else "Adjacent pages") + '">' + neighbour(previous, "← " + ("Précédent" if language == "fr" else "Previous")) + neighbour(following, ("Suivant" if language == "fr" else "Next") + " →") + "</nav>"
    return f'''<!doctype html>
<html lang="{language}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{esc(title)} — Aède</title><meta name="description" content="{esc(description)}"><meta name="robots" content="{'noindex,follow' if page.get('noindex') else 'index,follow'}">
<link rel="canonical" href="{esc(canonical)}"><link rel="alternate" hreflang="{language}" href="{esc(canonical)}"><link rel="alternate" hreflang="{other}" href="{esc(base_url + counterpart)}"><link rel="alternate" hreflang="x-default" href="{esc(base_url + 'docs/index.html')}">
<meta property="og:type" content="article"><meta property="og:title" content="{esc(title)} — Aède"><meta property="og:description" content="{esc(description)}"><meta property="og:url" content="{esc(canonical)}"><meta property="og:image" content="{esc(base_url + 'assets/og-image.png')}">
<meta name="theme-color" content="#121317"><link rel="icon" type="image/svg+xml" href="{esc(relative(output, 'assets/favicon.svg'))}"><link rel="stylesheet" href="{esc(asset_href(output, 'styles.css', asset_versions))}"><link rel="stylesheet" href="{esc(asset_href(output, 'guide.css', asset_versions))}"><script src="{esc(asset_href(output, 'guide.js', asset_versions))}" defer></script><script src="{esc(asset_href(output, 'explainers.js', asset_versions))}" defer></script>
</head><body class="guide-document" data-section="{esc(page.get('section', 'manual'))}"><a class="guide-skip" href="#main">{"Aller au contenu" if language == "fr" else "Skip to content"}</a>
<header class="guide-header"><a class="guide-logo" href="{esc(home)}" aria-label="Aède — {"accueil" if language == "fr" else "home"}">aède<span>.</span></a><a class="guide-header-label" href="{esc(relative(output, f'docs/{language}/index.html'))}">Documentation</a><div class="guide-header-actions"><a class="guide-project" href="{REPOSITORY}">{"Le projet" if language == "fr" else "Project"}</a><nav class="guide-language" aria-label="{"Langue" if language == "fr" else "Language"}"><a href="{esc(relative(output, counterpart))}" lang="{other}" hreflang="{other}" aria-label="{"Read in English" if other == "en" else "Lire en français"}" data-language="{other}" data-fragment-map="{esc(json.dumps(fragment_map, ensure_ascii=False))}">{other.upper()}</a></nav><button type="button" class="guide-menu" aria-controls="guide-sidebar" aria-expanded="false">Menu</button></div></header>
<div class="guide-layout"><aside class="guide-sidebar" id="guide-sidebar" aria-label="{"Navigation de la documentation" if language == "fr" else "Documentation navigation"}">{sidebar(pages, language, output, slug)}</aside>
<main class="guide-main" id="main"><p class="guide-eyebrow">{esc(section)}</p><h1 id="{esc(content.title_id)}">{esc(title)}</h1><p class="guide-description">{esc(description)}</p>{notice}{explainer}<article class="guide-prose"{body_lang}>{content.html}</article>{prev_next}<footer class="guide-source">{source_link}<p>{source_note}</p></footer></main>
<aside class="guide-toc"><p>{"Sur cette page" if language == "fr" else "On this page"}</p><nav aria-label="{"Sommaire" if language == "fr" else "Contents"}"><ul>{toc_links}</ul></nav></aside></div>
<footer class="guide-footer">aède<span>.</span> <a href="https://craft-and-code.github.io/FlacCompagnon/">FlacCompagnon ↗</a><a href="{REPOSITORY}">GitHub ↗</a>{privacy_footer}</footer></body></html>'''


def overview(pages: list[dict], language: str) -> Rendered:
    intro = "Commencez par l’installation et votre premier catalogue, puis choisissez le niveau de détail qui vous convient." if language == "fr" else "Start with installation and your first catalog, then choose the detail you need."
    cards = []
    for section, title in GROUPS[language]:
        candidates = [p for p in pages if p["section"] == section and not p.get("reference") and p.get("navigation", True)]
        visible = [p for p in pages if p["section"] == section and p.get("navigation", True)]
        if not visible: continue
        first = candidates[0] if candidates else visible[0]
        description = {"manual": ("Installer, démarrer, comprendre et préserver sa collection.", "Install, start, understand and preserve your collection."), "cli": ("Chaque commande, ses arguments, ses options et des exemples.", "Every command, its arguments, options and examples."), "server": ("Démarrer l’API locale et comprendre chaque route HTTP.", "Start the local API and understand every HTTP route."), "dsp": ("Comprendre le traitement audio, les mesures et leurs limites.", "Understand audio processing, measurements and their limits."), "compatibility": ("Adapter un lecteur à Aède et vérifier sa compatibilité.", "Adapt a player to Aède and verify its compatibility.")}[section][0 if language == "fr" else 1]
        cards.append(f'<a class="guide-card" data-card-section="{section}" href="{esc(first["slug"])}.html"><small>{len(candidates)} {"guides" if language == "fr" else "guides"}</small><h2>{title}</h2><p>{description}</p><span aria-hidden="true">→</span></a>')
    return Rendered(f'<p>{intro}</p><div class="guide-cards">{"".join(cards)}</div>', [], "documentation")


def localize_home(source: str, translations: dict, language: str) -> str:
    """Translate exact authored text while preserving markup and whitespace."""
    values = translations.get(language, {})
    def translated(value):
        leading = value[:len(value) - len(value.lstrip())]
        trailing = value[len(value.rstrip()):]
        plain = html.unescape(value.strip())
        return leading + esc(values[plain]) + trailing if plain in values else value
    pieces = re.split(r"(<[^>]*>)", source)
    skipped = None
    json_script = False
    for index, piece in enumerate(pieces):
        if piece.startswith("<"):
            if re.match(r"<script\b", piece, re.I):
                skipped = "script"; json_script = 'application/ld+json' in piece
            elif re.match(r"<style\b", piece, re.I): skipped = "style"
            elif re.match(r"</(?:script|style)\b", piece, re.I): skipped = None; json_script = False
            def attr(match):
                raw = html.unescape(match[2])
                return match[1] + '="' + esc(values.get(raw, raw)) + '"'
            piece = re.sub(r'\b(title|aria-label|placeholder|content)="([^"]*)"', attr, piece)
            pieces[index] = piece
        elif skipped == "script" and json_script:
            try:
                data = json.loads(piece)
                def walk(value):
                    if isinstance(value, dict): return {key: walk(item) for key, item in value.items()}
                    if isinstance(value, list): return [walk(item) for item in value]
                    return values.get(value, value) if isinstance(value, str) else value
                pieces[index] = json.dumps(walk(data), ensure_ascii=False, indent=2)
            except json.JSONDecodeError: pass
        elif not skipped:
            pieces[index] = translated(piece)
    result = "".join(pieces)
    return re.sub(r'<html\b[^>]*\blang="[^"]*"', '<html lang="' + language + '"', result, count=1)


@lru_cache(maxsize=1)
def project_stats_module():
    """Load the optional metrics helper without requiring packages or Cargo."""
    spec = importlib.util.spec_from_file_location("aede_site_project_stats", Path(__file__).resolve().parent / "project-stats.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def project_statistics(report: dict, language: str) -> str:
    """Generate tables/body text only; the Markdown source owns every heading."""
    french = language == "fr"
    source = report["source"]
    commits = report["commits"]
    def number(value): return f"{value:,}".replace(",", " ") if french else f"{value:,}"
    def cell(value): return str(value).replace("|", "\\|").replace("\n", " ").replace("\r", " ")
    def table(headers, rows):
        return "\n".join(["| " + " | ".join(cell(value) for value in headers) + " |", "| " + " | ".join("---" for _ in headers) + " |"] + ["| " + " | ".join(cell(value) for value in row) + " |" for row in rows])
    general = [
        ("Lignes physiques de source" if french else "Physical source lines", source["total"]["physical_lines"]),
        ("Lignes non vides" if french else "Nonblank lines", source["total"]["nonblank_lines"]),
        ("Fichiers source" if french else "Source files", source["total"]["files"]),
        ("Crates" if french else "Crates", len(source["crates"])),
        ("Pages publiées par langue" if french else "Published pages in this language", source["documentation"]["published_pages_by_language"][language]),
        ("Fichiers Markdown de documentation" if french else "Documentation Markdown files", source["documentation"]["markdown_files"]),
    ]
    result = [
        table(("Mesure" if french else "Metric", "Projet" if french else "Project"), [(label, number(value)) for label, value in general] + [
            ("Commits du projet" if french else "Project commits", commits["display"][language]),
        ]),
        "Les lignes physiques incluent les commentaires et les lignes vides. Les sources de test et les exemples restent distingués du code de production." if french else "Physical lines include comments and blank lines. Test sources and examples remain distinct from production code.",
        "Le nombre de commits couvre les ancêtres accessibles depuis la révision HEAD de cette publication, merges compris, sans compter les autres branches non fusionnées. Le seuil arrondi n’est publié que lorsque l’historique Git local est complet ; un historique partiel ou absent laisse le total indisponible. Aucun historique n’est récupéré sur le réseau pendant ce calcul." if french else "The commit count covers ancestors reachable from this publication’s HEAD revision, including merges, without counting other unmerged branches. The rounded lower bound is published only when the local Git history is complete; partial or missing history leaves the total unavailable. The calculation never fetches history over the network.",
        table(("Crate", "Production", "Tests", "Exemples" if french else "Examples", "Total"), [
            (crate["name"], *(number(crate[kind]["physical_lines"]) for kind in ("production", "tests", "examples", "total"))) for crate in source["crates"]
        ]),
        table(("Langage" if french else "Language", "Fichiers" if french else "Files", "Lignes physiques" if french else "Physical lines"), [
            (name, number(count["files"]), number(count["physical_lines"])) for name, count in source["by_language"].items()
        ]),
    ]
    unit = report["unit_tests"]
    if unit is None:
        result.append("**TU actifs : inventaire indisponible pour cette publication.** Aucun nombre n’est déduit des attributs `#[test]` dans les fichiers source." if french else "**Active unit tests: no inventory was supplied for this publication.** No count is inferred from source-file `#[test]` attributes.")
        result.append("Pour fournir un inventaire actuel :" if french else "To supply a current inventory:")
        result.append("```sh\npython3 tools/project-stats.py --tests --json --output target/project-stats.json\npython3 tools/build-site.py --project-stats target/project-stats.json --check\n```")
    else:
        result.append("**" + unit["display"][language] + "**")
        result.append("Ce seuil arrondi provient des tests enregistrés dans les exécutables Cargo de bibliothèque et de binaire, après exclusion des tests ignorés. Les tests d’intégration, doctests et tests des outils Python sont distincts. Le comptage n’exécute aucun corps de test et ne prouve pas leur réussite." if french else "This rounded lower bound comes from registered Cargo library/binary tests, excluding ignored tests. Integration tests, doctests and Python helper tests are separate. Listing does not execute test bodies or establish that they pass.")
        configuration = unit["configuration"]
        feature_label = "fonctionnalités par défaut" if french else "default features"
        if not configuration["default_features"]: feature_label = "sans fonctionnalités par défaut" if french else "without default features"
        extra = configuration.get("extra_features")
        if extra: feature_label += "; " + ("fonctionnalités supplémentaires : " if french else "extra features: ") + cell(extra)
        host = cell(configuration.get("rustc_host") or configuration["platform"])
        collected = cell(unit["provenance"]["collected_at"])
        result.append(("Inventaire TU : " if french else "Unit-test inventory: ") + f"`{host}`, {feature_label}; " + ("collecté le " if french else "collected at ") + f"`{collected}`.")
    provenance = report["provenance"]
    result.append(("Mesures des sources générées le " if french else "Source metrics generated at ") + f"`{cell(provenance['generated_at'])}`.")
    if provenance.get("revision"):
        result.append(("Révision de référence : " if french else "Reference revision: ") + f"`{cell(provenance['revision'])}` " + ("(les sources locales sont mesurées, y compris les modifications non commitées)." if french else "(local sources are measured, including uncommitted changes)."))
    result.append(("Empreinte des sources mesurées : " if french else "Measured source fingerprint: ") + f"`{provenance['source_fingerprint']}`.")
    return "\n\n".join(result)


def generated_page_sources(root: Path, pages: list[dict], stats_path=None) -> dict:
    generated = [page for page in pages if page.get("generated")]
    if not generated:
        if stats_path is not None: raise ValueError("A project statistics inventory was supplied but no statistics page is registered")
        return {}
    sources = {}
    for page in generated:
        for language in ("fr", "en"):
            path = page["source"][language]
            source = (root / path).read_text(encoding="utf-8")
            if source.count(PROJECT_STATS_MARKER) != 1:
                raise ValueError(f"{path}: project-statistics requires exactly one {PROJECT_STATS_MARKER} placeholder")
            sources[page["slug"], language] = source
    module = project_stats_module()
    try:
        report = module.collect(root)
        if stats_path is not None: report["unit_tests"] = module.load_test_inventory(Path(stats_path), root)
    except module.StatsError as error:
        raise ValueError(str(error)) from error
    return {key: source.replace(PROJECT_STATS_MARKER, project_statistics(report, key[1])) for key, source in sources.items()}


def build(root=ROOT, destination=None, base_url=BASE_URL, stats_path=None) -> list[dict]:
    destination = destination or root / "dist-site"
    if destination.resolve() == root.resolve() or root.resolve() in destination.resolve().parents and destination.resolve() != (root / "dist-site").resolve():
        raise ValueError("Refusing to replace an authored source directory; use dist-site")
    if destination.exists() and destination.resolve().parent != root.resolve() and not (destination / ".nojekyll").exists():
        raise ValueError("Refusing to replace an existing external directory without a generated-site marker")
    pages = load_pages(root)
    generated = generated_page_sources(root, pages, stats_path)
    if destination.exists(): shutil.rmtree(destination)
    shutil.copytree(root / "site", destination)
    for filename in ("README.md", "home-translations.json", "index.en.html"):
        (destination / filename).unlink(missing_ok=True)
    home_source = (root / "site/index.html").read_text(encoding="utf-8")
    translation_path = root / "site/home-translations.json"
    translations = json.loads(translation_path.read_text(encoding="utf-8")) if translation_path.exists() else {}
    (destination / "home-strings.js").write_text("window.aedeHomeTranslations = Object.freeze(" + json.dumps(translations, ensure_ascii=False).replace("<", "\\u003c") + ");\n", encoding="utf-8")
    # Content versions prevent stale landing-page scripts/styles after a rebuild.
    asset_versions = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()[:12]
                      for path in destination.iterdir() if path.is_file() and path.suffix in (".css", ".js")}
    home_urls = []
    for language in ("fr", "en"):
        output = "index.html" if language == "fr" else "en/index.html"
        source = (root / "site/index.en.html").read_text(encoding="utf-8") if language == "en" and (root / "site/index.en.html").exists() else localize_home(home_source, translations, language)
        source = re.sub(r'\b(href|src)="([^"?#]+)"',
                        lambda m: m[1] + '="' + m[2] + ('?v=' + asset_versions[m[2]] if m[2] in asset_versions else '') + '"', source)
        source = source.replace("docs/fr/", f"docs/{language}/")
        if language == "en":
            def switch_language(match):
                tag = match[0]
                tag = re.sub(r'href="[^"]*"', 'href="../"', tag)
                tag = re.sub(r'((?:lang|hreflang|data-lang)=)"en"', r'\1"fr"', tag)
                tag = re.sub(r'aria-label="[^"]*"', 'aria-label="Lire en français"', tag)
                return re.sub(r'>EN</a>', '>FR</a>', tag)
            source = re.sub(r'<a\b[^>]*class="ae-language"[^>]*>EN</a>', switch_language, source)
            def form_value(match):
                tag = match[0]
                field = re.search(r'name="([^"]+)"', tag)
                if not field: return tag
                if field[1] == "langue": return re.sub(r'value="fr"', 'value="en"', tag)
                if field[1] in ("_subject", "inscription"):
                    tag = re.sub(r'value="([^"]*)"', lambda value: 'value="' + esc(translations.get("en", {}).get(html.unescape(value[1]), html.unescape(value[1]))) + '"', tag)
                return tag
            source = re.sub(r'<input\b[^>]*>', form_value, source)
        if language == "en":
            # Only page-local assets are prefixed; HTTPS and hash navigation
            # retain their semantics, including form endpoints.
            source = re.sub(r'\b(href|src)="(?!https?:|mailto:|#|/|\.\./)([^"]+)"', lambda m: m[1] + '="../' + m[2] + '"', source)
        canonical = base_url + ("" if language == "fr" else "en/")
        source = re.sub(r'<link\b[^>]*rel="(?:canonical|alternate)"[^>]*>\s*', "", source)
        seo = f'<link rel="canonical" href="{esc(canonical)}"><link rel="alternate" hreflang="fr" href="{esc(base_url)}"><link rel="alternate" hreflang="en" href="{esc(base_url + "en/")}"><link rel="alternate" hreflang="x-default" href="{esc(base_url + "en/")}">'
        source = source.replace("</head>", seo + "</head>")
        source = re.sub(r'(<meta\b[^>]*property="og:url"[^>]*content=")[^"]*(")', lambda m: m[1] + canonical + m[2], source)
        source = re.sub(r'("url"\s*:\s*")https://craft-and-code.github.io/aede/(?:en/)?(")', lambda m: m[1] + canonical + m[2], source)
        target = destination / output; target.parent.mkdir(parents=True, exist_ok=True); target.write_text(source, encoding="utf-8")
        home_urls.append(canonical)
    urls = list(home_urls)
    for language in ("fr", "en"):
        for page in pages:
            output = f"docs/{language}/{page['slug']}.html"
            source_path = page["source"][language]
            renderer = Markdown(lambda href: rewrite_link(href, source_path, output, language, pages, root))
            try:
                source = generated.get((page["slug"], language))
                content = renderer.render(source if source is not None else (root / source_path).read_text(encoding="utf-8"))
            except ValueError as error:
                raise ValueError(f"{source_path}: {error}") from error
            target = destination / output; target.parent.mkdir(parents=True, exist_ok=True)
            other = "en" if language == "fr" else "fr"
            counterpart = generated.get((page["slug"], other))
            counterpart_content = Markdown().render(counterpart if counterpart is not None else (root / page["source"][other]).read_text(encoding="utf-8"))
            target.write_text(document_shell(page, language, content, pages, base_url, counterpart_content, asset_versions), encoding="utf-8")
            if not page.get("noindex"): urls.append(base_url + output)
        index_page = {"slug": "", "section": "manual", "title": {"en": "Documentation", "fr": "Documentation"}, "description": {"en": "Aède user manual, complete command reference, local server API, interactive audio DSP guides and player compatibility specifications.", "fr": "Manuel utilisateur Aède, référence complète des commandes, API du serveur local, guides interactifs du DSP audio et spécifications de compatibilité des lecteurs."}}
        (destination / f"docs/{language}/index.html").write_text(document_shell(index_page, language, overview(pages, language), pages, base_url, asset_versions=asset_versions), encoding="utf-8")
        urls.append(base_url + f"docs/{language}/index.html")
    (destination / "docs/index.html").write_text(f'<!doctype html><html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Documentation — Aède</title><meta name="description" content="Choisissez votre langue / Choose your language for the Aède documentation."><link rel="canonical" href="{esc(base_url + "docs/index.html")}"><link rel="alternate" hreflang="fr" href="{esc(base_url + "docs/fr/index.html")}"><link rel="alternate" hreflang="en" href="{esc(base_url + "docs/en/index.html")}"><link rel="stylesheet" href="{esc(asset_href("docs/index.html", "guide.css", asset_versions))}"></head><body class="guide-document"><main class="guide-locale"><a class="guide-logo" href="../">aède<span>.</span></a><h1>Documentation</h1><p>Choisissez votre langue / Choose your language</p><a href="fr/index.html" lang="fr">Français →</a><a href="en/index.html" lang="en">English →</a></main></body></html>', encoding="utf-8")
    urls.append(base_url + "docs/index.html")
    # Copy images/files linked from Markdown. Generated pages reference these
    # source assets without requiring handwritten copies in site/.
    for asset in (root / "docs").rglob("*"):
        if asset.is_file() and asset.suffix.lower() in (".png", ".jpg", ".jpeg", ".svg", ".gif", ".webp", ".pdf"):
            target = destination / "source-assets" / asset.relative_to(root)
            target.parent.mkdir(parents=True, exist_ok=True); shutil.copy2(asset, target)
    (destination / ".nojekyll").touch()
    (destination / "robots.txt").write_text(f"User-agent: *\nAllow: /\nSitemap: {base_url}sitemap.xml\n", encoding="utf-8")
    (destination / "sitemap.xml").write_text('<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' + "\n".join(f"<url><loc>{esc(url)}</loc></url>" for url in urls) + "\n</urlset>\n", encoding="utf-8")
    (destination / "site-pages.json").write_text(json.dumps({"pages": [{"slug": p["slug"], "section": p["section"], "command": p.get("command"), "source": p["source"], "explainer": p.get("explainer"), "noindex": p.get("noindex", False), "navigation": p.get("navigation", True)} for p in pages]}, indent=2) + "\n", encoding="utf-8")
    return pages


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist-site")
    parser.add_argument("--base-url", default=BASE_URL)
    parser.add_argument("--check", action="store_true", help="Validate generated metadata, local links and fragments")
    parser.add_argument("--project-stats", type=Path, help="Include a source-matching Rust unit-test inventory from project-stats.py")
    args = parser.parse_args()
    pages = build(destination=args.output, base_url=args.base_url.rstrip("/") + "/", stats_path=args.project_stats)
    print(f"Built {len(pages)} documentation pages in both languages into {args.output}")
    if args.check:
        import runpy
        checker = runpy.run_path(str(ROOT / "tools/check-site.py"))
        failures = checker["check"](args.output)
        if failures:
            print("\n".join(failures))
            raise SystemExit(1)
        print("Generated local links, fragments and SEO metadata are valid.")


if __name__ == "__main__":
    main()
