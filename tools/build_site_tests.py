#!/usr/bin/env python3
"""Behaviour checks for the dependency-free documentation publisher."""
from __future__ import annotations

import importlib.util
import json
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("aede_site_builder", TOOLS / "build-site.py")
builder = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = builder
SPEC.loader.exec_module(builder)
CHECK_SPEC = importlib.util.spec_from_file_location("aede_site_checker", TOOLS / "check-site.py")
checker = importlib.util.module_from_spec(CHECK_SPEC)
CHECK_SPEC.loader.exec_module(checker)


class MarkdownTests(unittest.TestCase):
    def render(self, source, rewrite=lambda href: href):
        return builder.Markdown(rewrite).render(source)

    def test_invisible_control_characters_are_rejected_but_tabs_are_kept(self):
        with self.assertRaisesRegex(ValueError, r"U\+0007"):
            self.render("```powershell\nC:\x07ede\n```")
        self.assertIn("Music", self.render("```text\nC:\\Music\n\tindent\n```").html)

    def test_headings_have_stable_unique_github_fragments(self):
        result = self.render("# Aède\n\n## Crêtes et `--gain`\n\n## Crêtes et `--gain`\n\n### Details!")
        self.assertEqual("aède", result.title_id)
        self.assertEqual([(2, "crêtes-et---gain", "Crêtes et --gain"), (2, "crêtes-et---gain-1", "Crêtes et --gain"), (3, "details", "Details!")], result.headings)
        self.assertNotIn("<h1", result.html)
        self.assertIn('id="crêtes-et---gain-1"', result.html)

    def test_code_preserves_literal_html_queries_and_newlines(self):
        result = self.render('# Title\n\n`<script>` and `rating:>=4`\n\n```sh\naede query "rating:>=4"\n<file> & --help\n```')
        self.assertIn('<code>&lt;script&gt;</code>', result.html)
        self.assertIn('rating:&gt;=4', result.html)
        self.assertIn('&lt;file&gt; &amp; --help\n</code>', result.html)
        self.assertNotIn('<script>', result.html)

    def test_tables_preserve_pipes_inside_code_and_escaped_pipes(self):
        result = self.render('| Option | Meaning |\n| :--- | ---: |\n| `off|track|album` | left \\| right |')
        self.assertIn('<code>off|track|album</code>', result.html)
        self.assertIn('left | right', result.html)
        self.assertIn('class="align-right"', result.html)
        self.assertEqual(result.html.count('<td'), 2)

    def test_nested_lists_and_continuation_paragraphs_remain_nested(self):
        result = self.render('1. Start\n   - Scan\n     - A folder\n   - Listen\n2. Finish\n\n- Item\n\n  Another paragraph\n- Next')
        self.assertIn('<ol>', result.html)
        self.assertIn('<ul><li><p>Scan</p>', result.html)
        self.assertIn('<li>A folder</li>', result.html)
        self.assertIn('<p>Another paragraph</p>', result.html)
        self.assertEqual(result.html.count('<ul>'), 3)

    def test_reference_like_lines_inside_code_remain_literal(self):
        result = self.render("```text\n[ref]: https://example.test\n```\n\nUnlinked [text][ref]")
        self.assertIn("[ref]: https://example.test", result.html)
        self.assertIn("Unlinked [text][ref]", result.html)

    def test_links_images_inline_markup_and_references(self):
        source = '**Bold** _italic_ ~~old~~ [`--help`](options.md#help)\n\n![Graph](plot.png "Diagram")\n\n[Site][ref]\n\n[ref]: https://example.test "Source"'
        result = self.render(source, lambda href: 'mapped/' + href if not href.startswith('https:') else href)
        self.assertIn('<strong>Bold</strong>', result.html)
        self.assertIn('<em>italic</em>', result.html)
        self.assertIn('<del>old</del>', result.html)
        self.assertIn('<a href="mapped/options.md#help"><code>--help</code></a>', result.html)
        self.assertIn('<img src="mapped/plot.png" alt="Graph" loading="lazy" title="Diagram">', result.html)
        self.assertIn('<a href="https://example.test" title="Source">Site</a>', result.html)

    def test_details_keep_authored_html_and_render_markdown_between_tags(self):
        result = self.render('<details>\n<summary>More</summary>\n\n## Inside\n\nA [reference](other.md).\n\n</details>', lambda href: href.replace('.md', '.html'))
        self.assertIn('<details>', result.html)
        self.assertIn('<summary>More</summary>', result.html)
        self.assertIn('<h2 id="inside">', result.html)
        self.assertIn('href="other.html"', result.html)
        self.assertTrue(result.html.endswith('</details>'))

    def test_task_lists_quotes_and_setext_headings(self):
        result = self.render('Title\n=====\n\n> **Remember**\n> Local files stay unchanged.\n\n- [x] Done\n- [ ] Next\n\nSubheading\n----------')
        self.assertIn('<blockquote><p><strong>Remember</strong>', result.html)
        self.assertIn('type="checkbox" disabled checked', result.html)
        self.assertIn('id="subheading"', result.html)


class LinkTests(unittest.TestCase):
    pages = [{'slug':'cli/query', 'section':'cli', 'source':{'en':'docs/cli/query.md','fr':'docs/fr/cli/query.md'}}, {'slug':'dsp/measurements', 'section':'dsp', 'source':{'en':'docs/imported-analyses.md','fr':'docs/fr/imported-analyses.md'}}]

    def test_published_markdown_links_stay_in_the_selected_locale(self):
        href = builder.rewrite_link('../imported-analyses.md#what-another-tool-found', 'docs/cli/query.md', 'docs/en/cli/query.html', 'en', self.pages)
        self.assertEqual('../dsp/measurements.html#what-another-tool-found', href)
        href = builder.rewrite_link('../../imported-analyses.md', 'docs/fr/cli/query.md', 'docs/fr/cli/query.html', 'fr', self.pages)
        self.assertEqual('../dsp/measurements.html', href)

    def test_english_fragments_follow_translated_target_headings(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'docs/fr').mkdir(parents=True)
            (root/'docs/imported-analyses.md').write_text('# What another tool found\n\n## Measurements\n', encoding="utf-8")
            (root/'docs/fr/imported-analyses.md').write_text('# Analyse importée\n\n## Mesures\n', encoding="utf-8")
            href = builder.rewrite_link('imported-analyses.md#what-another-tool-found', 'docs/library.md', 'docs/fr/manual/library.html', 'fr', self.pages, root)
            self.assertEqual('../dsp/measurements.html#analyse-import%C3%A9e', href)
            href = builder.rewrite_link('imported-analyses.md#measurements', 'docs/library.md', 'docs/fr/manual/library.html', 'fr', self.pages, root)
            self.assertEqual('../dsp/measurements.html#mesures', href)

    def test_engineering_links_refer_to_repository_sources(self):
        href = builder.rewrite_link('design/architecture.md#graph', 'docs/library.md', 'docs/fr/manual/library.html', 'fr', self.pages)
        self.assertEqual('https://github.com/craft-and-code/aede/blob/master/docs/design/architecture.md#graph', href)

    def test_source_edit_links_use_the_published_branch_and_selected_language(self):
        page = {'slug':'manual/first-steps', 'section':'manual', 'title':{'en':'First steps','fr':'Premiers pas'}, 'description':{'en':'Start using Aède','fr':'Commencer avec Aède'}, 'source':{'en':'docs/manual/first-steps.md','fr':'docs/fr/manual/first-steps.md'}}
        content = builder.Markdown().render('# First steps\n\nStart here.\n')
        for language, source in page['source'].items():
            with self.subTest(language=language):
                rendered = builder.document_shell(page, language, content, [page])
                self.assertIn(f'href="https://github.com/craft-and-code/aede/edit/master/{source}"', rendered)

    def test_external_and_fragment_links_remain_unchanged(self):
        for href in ('#usage','https://example.test/a#b','mailto:hello@example.test'):
            self.assertEqual(href,builder.rewrite_link(href,'docs/library.md','docs/en/manual/library.html','en',self.pages))


class HomeTests(unittest.TestCase):
    def test_home_publishes_compatibility_logos_and_verified_client_scope_in_both_languages(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "publication"
            builder.build(destination=destination)
            for language, home, label, key, scope in (
                ("fr", "index.html", "Compatible avec", "clé par application", "compatibilité à vérifier selon le lecteur"),
                ("en", "en/index.html", "Compatible with", "key for each application", "check which features your player supports"),
            ):
                with self.subTest(language=language):
                    source = (destination / home).read_text(encoding="utf-8")
                    section = re.search(r'<section id="ae-server"\s.*?</section>', source, re.S)
                    self.assertIsNotNone(section)
                    section = section[0]
                    self.assertIn("Subsonic/OpenSubsonic", section)
                    self.assertIn("Submariner", section)
                    self.assertIn(key, section)
                    self.assertIn(scope, section)
                    self.assertIn(label, section)
                    images = re.findall(r'<img\b[^>]*>', section)
                    self.assertLessEqual({"Subsonic", "OpenSubsonic"}, {re.search(r'\balt="([^"]+)"', tag)[1] for tag in images})
                    for tag in images:
                        src = re.search(r'\bsrc="([^"]+)"', tag)[1]
                        self.assertTrue((destination / home).parent.joinpath(src).is_file(), src)
                    self.assertNotIn("server/subsonic.html", section)
                    if language == "en":
                        self.assertNotIn("Configurer un lecteur", section)

    def test_exact_text_localization_preserves_nested_markup_and_code(self):
        source = '<html lang="fr"><head><title>Bonjour</title><meta name="description" content="Texte français"></head><body><p> Bonjour <strong>musique</strong> ! </p><button aria-label="Aller">Aller</button><script>const value="Bonjour";</script></body></html>'
        translated = builder.localize_home(source, {'en':{'Bonjour':'Hello','musique':'music','Aller':'Go','Texte français':'English text'}}, 'en')
        self.assertIn('<html lang="en">', translated)
        self.assertIn('<p> Hello <strong>music</strong> ! </p>', translated)
        self.assertIn('content="English text"',translated)
        self.assertIn('aria-label="Go">Go',translated)
        self.assertIn('const value="Bonjour"',translated)

    def test_structured_data_strings_are_localized_without_changing_keys(self):
        source = '<html lang="fr"><script type="application/ld+json">{"description":"Une bibliothèque","name":"Aède"}</script></html>'
        translated = builder.localize_home(source, {'en':{'Une bibliothèque':'A library'}}, 'en')
        self.assertIn('"description": "A library"',translated)
        self.assertIn('"name": "Aède"',translated)


class PublishTests(unittest.TestCase):
    def test_documentation_reloads_changed_assets_without_invalidating_unchanged_ones(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            destination = fixture.root / 'dist-site'
            assets = ('styles.css', 'guide.css', 'guide.js', 'explainers.js')
            documents = ('docs/en/index.html', 'docs/fr/index.html',
                         'docs/en/manual/project-statistics.html', 'docs/fr/manual/project-statistics.html')

            def versions(document):
                source = (destination / document).read_text(encoding='utf-8')
                urls = [url.rsplit('/', 1)[-1] for url in re.findall(r'(?:href|src)="([^"]+)"', source)]
                result = {}
                for asset in assets:
                    matches = [url for url in urls if url.split('?', 1)[0] == asset]
                    self.assertEqual(len(matches), 1, (document, asset))
                    self.assertRegex(matches[0], r'\?v=[a-f0-9]+$')
                    result[asset] = matches[0]
                return result

            builder.build(fixture.root)
            initial = versions(documents[0])
            for document in documents[1:]:
                self.assertEqual(versions(document), initial)
            chooser = (destination / 'docs/index.html').read_text(encoding='utf-8')
            self.assertIn(initial['guide.css'], chooser)
            builder.build(fixture.root)
            self.assertEqual(versions(documents[0]), initial)
            fixture.write('site/guide.css', '/* Updated field and menu styles */')
            fixture.write('site/guide.js', '/* Updated documentation navigation */')
            builder.build(fixture.root)
            updated = versions(documents[0])
            for asset in ('guide.css', 'guide.js'):
                self.assertNotEqual(updated[asset], initial[asset])
            for asset in ('styles.css', 'explainers.js'):
                self.assertEqual(updated[asset], initial[asset])
            for document in documents[1:]:
                self.assertEqual(versions(document), updated)
            self.assertIn(updated['guide.css'], (destination / 'docs/index.html').read_text(encoding='utf-8'))
            self.assertEqual([], checker.check(destination))

    def test_build_produces_bilingual_source_pages_and_valid_internal_links(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); (root/'site/assets').mkdir(parents=True); (root/'docs').mkdir()
            (root/'site/index.html').write_text('<html lang="fr"><head><title>Accueil</title><meta name="description" content="Accueil Aède"><link rel="stylesheet" href="styles.css"><script src="app.js" defer></script></head><body><a class="ae-language" href="en/" lang="en" hreflang="en" data-lang="en" aria-label="Read in English">EN</a><a href="docs/fr/index.html">Documentation</a></body></html>', encoding="utf-8")
            (root/'site/home-translations.json').write_text(json.dumps({'en':{'Accueil':'Home','Accueil Aède':'Aède home'}}), encoding="utf-8")
            for asset in ('app.js','styles.css','guide.css','guide.js','explainers.js','assets/favicon.svg','assets/og-image.png'):
                (root/'site'/asset).write_text('', encoding="utf-8")
            pages=[]
            for section in ('manual','cli','server','dsp'):
                for language in ('en','fr'):
                    path=f'docs/{language}/{section}.md'; (root/path).parent.mkdir(parents=True,exist_ok=True); (root/path).write_text(f'# {section}\n\n## Usage\n\n[Other page](dsp.md#usage)\n', encoding="utf-8")
                pages.append({'slug':f'{section}/start','section':section,'title':{'en':section,'fr':section},'description':{'en':f'{section} guide','fr':f'Guide {section}'},'source':{'en':f'docs/en/{section}.md','fr':f'docs/fr/{section}.md'}})
            for language in ('en','fr'):
                (root/f'docs/{language}/confirmation.md').write_text('# Confirmation\n\nRequest received.\n', encoding="utf-8")
            pages.append({'slug':'manual/subscribed','section':'manual','title':{'en':'Confirmation','fr':'Confirmation'},'description':{'en':'Confirmation page','fr':'Page de confirmation'},'source':{'en':'docs/en/confirmation.md','fr':'docs/fr/confirmation.md'},'navigation':False,'noindex':True})
            (root/'docs/site-manual-cli.json').write_text(json.dumps(pages), encoding="utf-8")
            destination=root/'dist-site'; builder.build(root,destination)
            self.assertFalse((destination/'home-translations.json').exists())
            self.assertIn('window.aedeHomeTranslations', (destination/'home-strings.js').read_text(encoding="utf-8"))
            self.assertIn('noindex,follow', (destination/'docs/fr/manual/subscribed.html').read_text(encoding="utf-8"))
            self.assertNotIn('manual/subscribed.html', (destination/'sitemap.xml').read_text(encoding="utf-8"))
            self.assertNotIn('Confirmation', builder.sidebar(pages, 'fr', 'docs/fr/index.html'))
            self.assertEqual([],checker.check(destination))
            french=(destination/'docs/fr/cli/start.html').read_text(encoding="utf-8")
            self.assertIn('href="../dsp/start.html#usage"',french)
            self.assertIn('aria-current="page"',french)
            self.assertIn('data-nav-section="cli" open',french)
            self.assertIn('href="../"', (destination/'en/index.html').read_text(encoding="utf-8"))
            self.assertIn('>FR</a>', (destination/'en/index.html').read_text(encoding="utf-8"))
            # A revised animation must load even when a browser caches the old script.
            home = (destination/'index.html').read_text(encoding="utf-8")
            asset_url = re.search(r'src="(app\.js\?v=[a-f0-9]+)"', home)
            self.assertIsNotNone(asset_url)
            asset_url = asset_url[1]
            self.assertIn('src="../' + asset_url + '"', (destination/'en/index.html').read_text(encoding="utf-8"))
            self.assertRegex(home, r'href="styles\.css\?v=[a-f0-9]+"')
            builder.build(root, destination)
            self.assertIn('src="' + asset_url + '"', (destination/'index.html').read_text(encoding="utf-8"))
            (root/'site/app.js').write_text('/* A new animation */', encoding="utf-8")
            builder.build(root, destination)
            self.assertNotIn('src="' + asset_url + '"', (destination/'index.html').read_text(encoding="utf-8"))
            self.assertEqual('/* A new animation */', (destination/'app.js').read_text(encoding="utf-8"))
            self.assertEqual([], checker.check(destination))

    def test_checker_rejects_missing_fragments_and_duplicate_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'index.html').write_text('<html lang="en"><head><title>Aède</title><meta name="description" content="Guide"><link rel="canonical" href="https://example.test/"><link rel="alternate" hreflang="en" href="https://example.test/"><link rel="alternate" hreflang="fr" href="https://example.test/"></head><body><h1 id="same">A</h1><h2 id="same">B</h2><a href="#missing">Missing</a></body></html>', encoding="utf-8")
            failures=checker.check(root)
            self.assertTrue(any('missing fragment' in failure for failure in failures))
            self.assertTrue(any('duplicate IDs' in failure for failure in failures))

    def test_third_hreflang_does_not_hide_a_missing_counterpart(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'index.html').write_text('<html lang="fr"><head><title>Aède</title><meta name="description" content="Guide"><link rel="canonical" href="https://example.test/"><link rel="alternate" hreflang="fr" href="https://example.test/"><link rel="alternate" hreflang="de" href="https://example.test/"></head></html>', encoding="utf-8")
            failures = checker.check(root)
            self.assertTrue(any('missing bilingual hreflang' in failure for failure in failures))

    def test_source_directory_cannot_be_used_as_build_destination(self):
        for path in ('docs','docs/fr/manual','site/assets','tools/nested','crates/aede-core','docs/dist-site'):
            with self.subTest(path=path):
                with self.assertRaises(ValueError): builder.build(destination=builder.ROOT/path)


class StatisticsFixture:
    def __init__(self, root):
        self.root = Path(root)
        self.write("site/index.html", '<html lang="fr"><head><title>Aède</title><meta name="description" content="Accueil"></head><body><a href="docs/fr/index.html">Documentation</a></body></html>')
        for asset in ("styles.css", "guide.css", "guide.js", "explainers.js", "assets/favicon.svg", "assets/og-image.png"):
            self.write("site/" + asset, "")
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/aede-core"]\n')
        self.write("crates/aede-core/Cargo.toml", '[package]\nname = "aede-core"\nversion = "0.1.0"\n')
        self.write("crates/aede-core/src/lib.rs", "// Core source\n\npub fn run() {}\n")
        # An orphan source count must never become an advertised active TU count.
        self.write("crates/aede-core/src/orphan_tests.rs", "#[test]\nfn unregistered() {}\n")
        self.write("docs/en/stats.md", "# Project statistics\n\n## Current snapshot\n\n<!-- project-statistics -->\n\n## Method\n\nAuthored explanation.\n")
        self.write("docs/fr/stats.md", "# Statistiques du projet\n\n## Instantané actuel\n\n<!-- project-statistics -->\n\n## Méthode\n\nExplication rédigée.\n")
        self.pages = [{"slug": "manual/project-statistics", "section": "manual", "generated": "project-statistics", "title": {"en": "Project statistics", "fr": "Statistiques du projet"}, "description": {"en": "Measured source statistics", "fr": "Mesures des sources"}, "source": {"en": "docs/en/stats.md", "fr": "docs/fr/stats.md"}}]
        self.save_manifest()

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

    def save_manifest(self):
        self.write("docs/site-project.json", json.dumps(self.pages))

    def inventory(self):
        module = builder.project_stats_module()
        report = module.collect(self.root)
        # Intentionally stale source totals prove the publisher remeasures code.
        report["source"]["total"]["physical_lines"] = 999999
        report["commits"] = {"status": "complete", "display": {"en": "> 98,700 commits", "fr": "+ de 98 700 commits"}}
        report["unit_tests"] = {
            "kind": "libtest_inventory", "active": 1567, "ignored": 0, "total": 1567,
            "lower_bound": 1500, "display": module.public_labels(1567),
            "by_crate": [{"name": "aede-core", "active": 1567, "ignored": 0, "total": 1567, "targets": ["aede_core"]}],
            "configuration": {"default_features": True, "extra_features": "", "rustc_host": "x86_64-unknown-linux-gnu", "platform": "linux"},
            "provenance": {"rust_source_fingerprint": module.rust_fingerprint(self.root), "executed_tests": False, "collected_at": "2026-10-03T08:00:00+00:00"},
        }
        return self.write("stats.json", json.dumps(report))


class ProjectStatisticsTests(unittest.TestCase):
    def test_bilingual_publication_preserves_unicode_when_the_platform_default_is_cp1252(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            fixture.write("site/index.html", '<html lang="fr"><head><title>Bibliothèque 音楽</title><meta name="description" content="Accueil Aède"></head><body><p>Bibliothèque 音楽</p></body></html>')
            fixture.write("site/home-translations.json", json.dumps({"en": {"Bibliothèque 音楽": "Music library 音楽"}}, ensure_ascii=False))
            fixture.write("docs/fr/stats.md", "# Statistiques du projet\n\n## Instantané actuel\n\nBibliothèque 音楽 et musique ♫.\n\n<!-- project-statistics -->\n")
            fixture.write("docs/en/stats.md", "# Project statistics\n\n## Current snapshot\n\nMusic library 音楽 and music ♫.\n\n<!-- project-statistics -->\n")
            native_open = Path.open

            def windows_text_default(path, mode="r", buffering=-1, encoding=None, errors=None, newline=None):
                if "b" not in mode and encoding is None:
                    encoding = "cp1252"
                return native_open(path, mode, buffering, encoding, errors, newline)

            with patch.object(Path, "open", windows_text_default):
                builder.build(fixture.root)
                destination = fixture.root / "dist-site"
                french = (destination / "docs/fr/manual/project-statistics.html").read_text(encoding="utf-8")
                english = (destination / "docs/en/manual/project-statistics.html").read_text(encoding="utf-8")
                self.assertIn("Bibliothèque 音楽 et musique ♫.", french)
                self.assertIn("Music library 音楽 and music ♫.", english)
                self.assertIn("Music library 音楽", (destination / "en/index.html").read_text(encoding="utf-8"))
                self.assertEqual([], checker.check(destination))

    def test_optional_documentation_groups_are_hidden_until_they_have_visible_pages(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            for language in ("en", "fr"):
                overview = builder.overview(fixture.pages, language).html
                navigation = builder.sidebar(fixture.pages, language, f"docs/{language}/index.html")
                self.assertIn('data-card-section="manual"', overview)
                self.assertNotIn('data-card-section="compatibility"', overview)
                self.assertNotIn('data-nav-section="compatibility"', navigation)
                self.assertNotIn('data-nav-section="cli"', navigation)
            page = {"slug": "compatibility/specification", "section": "compatibility", "title": {"en": "Player requirements", "fr": "Exigences du lecteur"}}
            fixture.pages.append(page)
            for language in ("en", "fr"):
                self.assertIn('data-card-section="compatibility"', builder.overview(fixture.pages, language).html)
                self.assertIn("Compatible Aède", builder.sidebar(fixture.pages, language, f"docs/{language}/index.html"))

    def test_fresh_source_tables_are_collected_once_without_inventing_tests_or_generated_headings(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            module = builder.project_stats_module()
            with patch.object(module, "collect", wraps=module.collect) as collect, patch.object(module, "inventory", side_effect=AssertionError("Website publication must never build tests")):
                builder.build(fixture.root)
            collect.assert_called_once_with(fixture.root)
            french = (fixture.root / "dist-site/docs/fr/manual/project-statistics.html").read_text(encoding="utf-8")
            english = (fixture.root / "dist-site/docs/en/manual/project-statistics.html").read_text(encoding="utf-8")
            self.assertIn("TU actifs : inventaire indisponible", french)
            self.assertIn("no inventory was supplied", english)
            self.assertIn("aede-core", french)
            self.assertIn("Lignes physiques de source", french)
            self.assertIn("Physical source lines", english)
            self.assertNotIn("<!-- project-statistics -->", french)
            self.assertIn('id="instantané-actuel"', french)
            self.assertIn('current-snapshot', french)
            self.assertIn('id="current-snapshot"', english)
            self.assertEqual(2, french.count('<h2 id="'))
            self.assertEqual([], checker.check(fixture.root / "dist-site"))

    def test_valid_inventory_publishes_only_rounded_bilingual_tu_counts_with_fresh_source_totals(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            snapshot = fixture.inventory()
            builder.build(fixture.root, stats_path=snapshot)
            french = (fixture.root / "dist-site/docs/fr/manual/project-statistics.html").read_text(encoding="utf-8")
            english = (fixture.root / "dist-site/docs/en/manual/project-statistics.html").read_text(encoding="utf-8")
            self.assertIn("+ de 1 500 TU actifs", french)
            self.assertIn("&gt; 1,500 active unit tests", english)
            for content in (french, english):
                self.assertIn("x86_64-unknown-linux-gnu", content)
                self.assertIn("2026-10-03T08:00:00+00:00", content)
                self.assertNotIn("1567 active", content)
                self.assertNotIn("1,567 active", content)
                self.assertNotIn("1 567 TU", content)
                self.assertNotIn("999,999", content)
                self.assertNotIn("999 999", content)
                self.assertNotIn("98,700 commits", content)
                self.assertNotIn("98 700 commits", content)

    def test_complete_commit_history_publishes_only_a_rounded_bilingual_total(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            module = builder.project_stats_module()
            commits = {
                "status": "complete", "reachable_from_head": 2345,
                "lower_bound": 2300,
                "display": {"en": "> 2,300 commits", "fr": "+ de 2 300 commits"},
                "revision": "abcdefabcdefabcdefabcdefabcdefabcdefabcd", "reason": None,
            }
            with patch.object(module, "git_history", return_value=commits):
                builder.build(fixture.root)
            french = (fixture.root / "dist-site/docs/fr/manual/project-statistics.html").read_text(encoding="utf-8")
            english = (fixture.root / "dist-site/docs/en/manual/project-statistics.html").read_text(encoding="utf-8")
            self.assertIn("+ de 2 300 commits", french)
            self.assertIn("&gt; 2,300 commits", english)
            self.assertIn("accessibles depuis", french)
            self.assertIn("reachable from", english)
            for content in (french, english):
                self.assertIn("HEAD", content)
                self.assertNotIn("2345", content)
                self.assertNotIn("2,345", content)
                self.assertNotIn("2 345", content)
                self.assertIn(commits["revision"], content)

    def test_partial_or_missing_commit_history_never_publishes_a_complete_total(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            module = builder.project_stats_module()
            for status, labels in (
                ("partial", {"en": "Unavailable (shallow history)", "fr": "Indisponible (historique partiel)"}),
                ("unavailable", {"en": "Unavailable (Git history missing)", "fr": "Indisponible (historique Git absent)"}),
            ):
                with self.subTest(status=status):
                    commits = {"status": status, "reachable_from_head": 17 if status == "partial" else None,
                               "lower_bound": None, "display": labels,
                               "revision": None, "reason": status}
                    with patch.object(module, "git_history", return_value=commits):
                        builder.build(fixture.root)
                    for language in ("en", "fr"):
                        content = (fixture.root / f"dist-site/docs/{language}/manual/project-statistics.html").read_text(encoding="utf-8")
                        self.assertIn(labels[language], content)
                        self.assertNotIn("+ de", content)
                        self.assertNotIn("&gt; 10 commits", content)
                        self.assertNotIn("17 commits", content)

    def test_stale_inventory_refuses_publication_and_preserves_the_existing_generated_site(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            snapshot = fixture.inventory()
            builder.build(fixture.root, stats_path=snapshot)
            preserved = fixture.write("dist-site/previous-publication.txt", "Existing publication\n")
            fixture.write("crates/aede-core/src/lib.rs", "pub fn changed() {}\n")
            with self.assertRaisesRegex(ValueError, "stale"):
                builder.build(fixture.root, stats_path=snapshot)
            self.assertEqual("Existing publication\n", preserved.read_text(encoding="utf-8"))

    def test_unknown_generated_content_and_missing_or_repeated_markers_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = StatisticsFixture(directory)
            fixture.pages[0]["generated"] = "unknown-content"
            fixture.save_manifest()
            with self.assertRaisesRegex(ValueError, "Unknown generated page content"):
                builder.build(fixture.root)
            fixture.pages[0]["generated"] = "project-statistics"
            fixture.save_manifest()
            for marker in ("", "<!-- project-statistics -->\n<!-- project-statistics -->"):
                fixture.write("docs/fr/stats.md", "# Statistiques\n\n" + marker + "\n")
                with self.subTest(marker=marker), self.assertRaisesRegex(ValueError, "exactly one"):
                    builder.build(fixture.root)


@unittest.skipUnless(shutil.which('node'), 'Node is optional; the static publisher itself only requires Python')
class DspModelTests(unittest.TestCase):
    def test_normalization_tone_and_downmix_obey_documented_bounds(self):
        source=(TOOLS.parent/'site/explainers.js').read_text(encoding="utf-8")
        program='''const assert=require('node:assert/strict'); const vm=require('node:vm'); const window={matchMedia:()=>({matches:true})}; const document={documentElement:{lang:'en'},querySelectorAll:()=>[]}; vm.runInNewContext(SOURCE,{window,document}); const m=window.AedeDspModels; assert.equal(m.safeGain(-24,-18,-3),3); assert.equal(m.safeGain(-10,-18,-1),-8); assert.equal(m.reserve(6,3),9); assert.equal(m.reserve(-6,3),3); assert.equal(m.downmix('LFE')[0],0); assert.equal(m.downmix('LFE')[1],0); assert.ok(Math.abs(m.downmix('FL')[0]+m.downmix('FC')[0]+m.downmix('SL')[0]-1)<1e-12); for(const phase of [0,.2,.6]){let peak=0;for(let i=0;i<10000;i++)peak=Math.max(peak,Math.abs(m.sampleSignal(i/10000,phase)));assert.ok(peak<=1+1e-12);assert.ok(peak>.99999);} assert.equal(m.toneResponse(1000,0,0),0); assert.ok(Math.abs(m.toneResponse(20,6,0))<.1); assert.ok(m.toneResponse(20000,6,0)<-5.9);'''.replace('SOURCE',json.dumps(source))
        result=subprocess.run(['node'],input=program,capture_output=True,text=True,encoding="utf-8")
        self.assertEqual(0,result.returncode,result.stderr)


if __name__=='__main__': unittest.main()
