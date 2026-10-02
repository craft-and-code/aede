#!/usr/bin/env python3
"""Behaviour checks for the dependency-free documentation publisher."""
from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

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
        with self.assertRaisesRegex(ValueError, "U\+0007"):
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
            (root/'docs/imported-analyses.md').write_text('# What another tool found\n\n## Measurements\n')
            (root/'docs/fr/imported-analyses.md').write_text('# Analyse importée\n\n## Mesures\n')
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
    def test_build_produces_bilingual_source_pages_and_valid_internal_links(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); (root/'site/assets').mkdir(parents=True); (root/'docs').mkdir()
            (root/'site/index.html').write_text('<html lang="fr"><head><title>Accueil</title><meta name="description" content="Accueil Aède"></head><body><a class="ae-language" href="en/" lang="en" hreflang="en" data-lang="en" aria-label="Read in English">EN</a><a href="docs/fr/index.html">Documentation</a></body></html>')
            (root/'site/home-translations.json').write_text(json.dumps({'en':{'Accueil':'Home','Accueil Aède':'Aède home'}}))
            for asset in ('styles.css','guide.css','guide.js','explainers.js','assets/favicon.svg','assets/og-image.png'):
                (root/'site'/asset).write_text('')
            pages=[]
            for section in ('manual','cli','server','dsp'):
                for language in ('en','fr'):
                    path=f'docs/{language}/{section}.md'; (root/path).parent.mkdir(parents=True,exist_ok=True); (root/path).write_text(f'# {section}\n\n## Usage\n\n[Other page](dsp.md#usage)\n')
                pages.append({'slug':f'{section}/start','section':section,'title':{'en':section,'fr':section},'description':{'en':f'{section} guide','fr':f'Guide {section}'},'source':{'en':f'docs/en/{section}.md','fr':f'docs/fr/{section}.md'}})
            for language in ('en','fr'):
                (root/f'docs/{language}/confirmation.md').write_text('# Confirmation\n\nRequest received.\n')
            pages.append({'slug':'manual/subscribed','section':'manual','title':{'en':'Confirmation','fr':'Confirmation'},'description':{'en':'Confirmation page','fr':'Page de confirmation'},'source':{'en':'docs/en/confirmation.md','fr':'docs/fr/confirmation.md'},'navigation':False,'noindex':True})
            (root/'docs/site-manual-cli.json').write_text(json.dumps(pages))
            destination=root/'dist-site'; builder.build(root,destination)
            self.assertFalse((destination/'home-translations.json').exists())
            self.assertIn('window.aedeHomeTranslations', (destination/'home-strings.js').read_text())
            self.assertIn('noindex,follow', (destination/'docs/fr/manual/subscribed.html').read_text())
            self.assertNotIn('manual/subscribed.html', (destination/'sitemap.xml').read_text())
            self.assertNotIn('Confirmation', builder.sidebar(pages, 'fr', 'docs/fr/index.html'))
            self.assertEqual([],checker.check(destination))
            french=(destination/'docs/fr/cli/start.html').read_text()
            self.assertIn('href="../dsp/start.html#usage"',french)
            self.assertIn('aria-current="page"',french)
            self.assertIn('data-nav-section="cli" open',french)
            self.assertIn('href="../"', (destination/'en/index.html').read_text())
            self.assertIn('>FR</a>', (destination/'en/index.html').read_text())

    def test_checker_rejects_missing_fragments_and_duplicate_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'index.html').write_text('<html lang="en"><head><title>Aède</title><meta name="description" content="Guide"><link rel="canonical" href="https://example.test/"><link rel="alternate" hreflang="en" href="https://example.test/"><link rel="alternate" hreflang="fr" href="https://example.test/"></head><body><h1 id="same">A</h1><h2 id="same">B</h2><a href="#missing">Missing</a></body></html>')
            failures=checker.check(root)
            self.assertTrue(any('missing fragment' in failure for failure in failures))
            self.assertTrue(any('duplicate IDs' in failure for failure in failures))

    def test_third_hreflang_does_not_hide_a_missing_counterpart(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'index.html').write_text('<html lang="fr"><head><title>Aède</title><meta name="description" content="Guide"><link rel="canonical" href="https://example.test/"><link rel="alternate" hreflang="fr" href="https://example.test/"><link rel="alternate" hreflang="de" href="https://example.test/"></head></html>')
            failures = checker.check(root)
            self.assertTrue(any('missing bilingual hreflang' in failure for failure in failures))

    def test_source_directory_cannot_be_used_as_build_destination(self):
        for path in ('docs','docs/fr/manual','site/assets','tools/nested','crates/aede-core','docs/dist-site'):
            with self.subTest(path=path):
                with self.assertRaises(ValueError): builder.build(destination=builder.ROOT/path)


@unittest.skipUnless(shutil.which('node'), 'Node is optional; the static publisher itself only requires Python')
class DspModelTests(unittest.TestCase):
    def test_normalization_tone_and_downmix_obey_documented_bounds(self):
        source=(TOOLS.parent/'site/explainers.js').read_text()
        program='''const assert=require('node:assert/strict'); const vm=require('node:vm'); const window={matchMedia:()=>({matches:true})}; const document={documentElement:{lang:'en'},querySelectorAll:()=>[]}; vm.runInNewContext(SOURCE,{window,document}); const m=window.AedeDspModels; assert.equal(m.safeGain(-24,-18,-3),3); assert.equal(m.safeGain(-10,-18,-1),-8); assert.equal(m.reserve(6,3),9); assert.equal(m.reserve(-6,3),3); assert.equal(m.downmix('LFE')[0],0); assert.equal(m.downmix('LFE')[1],0); assert.ok(Math.abs(m.downmix('FL')[0]+m.downmix('FC')[0]+m.downmix('SL')[0]-1)<1e-12); for(const phase of [0,.2,.6]){let peak=0;for(let i=0;i<10000;i++)peak=Math.max(peak,Math.abs(m.sampleSignal(i/10000,phase)));assert.ok(peak<=1+1e-12);assert.ok(peak>.99999);} assert.equal(m.toneResponse(1000,0,0),0); assert.ok(Math.abs(m.toneResponse(20,6,0))<.1); assert.ok(m.toneResponse(20000,6,0)<-5.9);'''.replace('SOURCE',json.dumps(source))
        result=subprocess.run(['node','-e',program],capture_output=True,text=True)
        self.assertEqual(0,result.returncode,result.stderr)


if __name__=='__main__': unittest.main()
