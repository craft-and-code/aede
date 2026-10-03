# Website source and publishing

`site/` contains the authored landing-page template, styles, scripts and assets. User documentation is authored only in Markdown under `docs/` and `docs/fr/`; the publisher renders those files into `dist-site/`. Do not maintain a second handwritten HTML copy of a guide.

## Build and preview

The publisher and local preview use the Python standard library (Python 3.9 or later). No package installation or network request is needed.

```sh
python3 tools/build_site_tests.py
python3 tools/build-site.py --check
python3 tools/preview-site.py
```

Open <http://127.0.0.1:4174/> for French or <http://127.0.0.1:4174/en/> for English. The documentation starts at `/docs/fr/` and `/docs/en/`; `/docs/` is an explicit language chooser. Serve `dist-site/` rather than opening the source template with `file://`.

`dist-site/` is generated and ignored. Edit `site/`, the Markdown sources or the documentation manifests, then rebuild. The `--output` option accepts a fresh output directory; the default is `dist-site/`. The publisher refuses to replace authored repository directories.

## Documentation catalog

`docs/site-manual-cli.json`, `docs/site-server-dsp.json` and any further `docs/site-*.json` manifest declare the published pages. Each entry provides:

- `slug`: stable, lowercase URL path without `.html`, for example `cli/query`;
- `section`: `manual`, `cli`, `server`, `compatibility` or `dsp`; `compatibility` is the dedicated **Compatible Aède** section for client developers;
- `title` and `description`: English and French metadata;
- `source`: the English and French Markdown paths;
- optional `command` and `group`: command label and sidebar category;
- optional `explainer`: an interactive DSP illustration identifier.
- optional `generated`: `project-statistics` for the statistics page, with exactly one `<!-- project-statistics -->` marker in each authored translation.

`docs/site-project.json` registers the generated project statistics and client specification. The statistics text explains the measurement rules; its tables are rendered from fresh source measurements. The publisher never maintains numeric claims in authored Markdown. To include the rounded active-TU display, supply a compiled test inventory:

```sh
python3 tools/project_stats_tests.py
python3 tools/project-stats.py --tests --output target/project-stats.json
python3 tools/build-site.py --check --project-stats target/project-stats.json
```

The inventory builds existing library/binary test harnesses offline, then lists active tests without executing them. It requires the normal Cargo/native build prerequisites and already fetched dependencies. A stale Rust/Cargo fingerprint is refused. Without `--project-stats`, source tables remain current and the active-TU display is explicitly unavailable; no count is inferred from test attributes. The [statistics guide](../docs/manual/project-statistics.md) defines inclusions, exclusions and feature/platform provenance. Both the Site workflow and `tools/check.sh` generate an inventory before publishing the tables.

The publisher also includes the existing user-facing topic references without copying their text. When a reference has no French translation, its French navigation page explicitly labels the article as English and marks the article language accordingly. Engineering and design links continue to open the corresponding versioned repository source. Relative links between published Markdown sources are rewritten to the selected locale, with fragments preserved or mapped to corresponding translated headings.

Translated topic references preserve their original English heading fragments as empty `<div id="original-fragment" data-legacy-anchor></div>` anchors before the translated headings. Keep those anchors when editing so that existing deep links continue to resolve. The website and repository documentation checks validate these explicit IDs.

The first Markdown H1 supplies the article title fragment; manifest metadata supplies its displayed title. Headings use stable GitHub-style IDs, including duplicate suffixes. Supported authored Markdown includes ATX/setext headings, fenced and indented code, paragraphs, nested ordered/unordered and task lists, blockquotes, GFM tables, inline code/emphasis/strikethrough, inline/reference links, images, autolinks and repository-authored HTML such as `details`/`summary`. This renderer is intended only for trusted documentation in the repository, never for visitor input. It is not an HTML sanitizer or a complete CommonMark implementation.

## Landing-page languages

`site/index.html` is the single French landing-page source. `site/home-translations.json` maps exact French strings to their English equivalents. The build translates text nodes, human-readable attributes and JSON-LD strings while preserving the HTML structure. `site/translations.js` uses the generated `home-strings.js` dictionary for dynamic landing-page labels; there is no second handwritten translation list. The generated English home uses localized documentation links and assets relative to its `/en/` path.

Language choices are explicit on documentation pages, point to the same guide in the other language, and save the shared `aede-language` preference. Navigation uses ordinary page links; scripts enhance the page but are not required to read it.

Repository references use the published `master` branch, declared once as `REPOSITORY_BRANCH` in the publisher. The Markdown source link opens GitHub's editor for the page's selected language; unpublished local source changes become available there when pushed to that branch.

## Navigation behaviour

`guide.js` stores the sidebar's scroll position and expanded groups in session storage before navigation and restores them after layout. It opens the active page's group without resetting the menu to its top, making only the smallest scroll adjustment if the active item would otherwise fall outside the visible menu (on a first visit or after changing language). Ordinary menu links retain the exact saved position. Searching filters command/topic labels without losing the pre-search expanded-group state. The mobile menu supports keyboard focus and Escape. Fragment navigation opens any enclosing `details` elements before scrolling. Code blocks offer clipboard copying when the browser permits it, with a terminal header for shell examples.

## DSP illustrations

`explainers.js` provides synthetic interactive models for the PCM path, normalization/headroom, source intersample peaks, broad shelves, stereo downmix, rate conversion, integer dither, compatible or reset track joins and peak/spectrum observation. Controls update explanatory values and accessible readouts. The illustrations never play audio, access user files or claim to reproduce every internal detail of the Rust engine. They animate only while visible, stop when the document is hidden, and become static under `prefers-reduced-motion` while retaining interactive controls.

## Verification and GitHub Pages

`tools/check-site.py dist-site` validates local files, heading fragments, unique IDs, descriptions, canonical URLs and bilingual `hreflang` counterparts. `build-site.py --check` runs the same validation immediately after publishing. The default checker permits only the core RustDoc entry to be missing before the separate Cargo documentation step.

The Pages workflow generates the site, builds RustDoc without dependencies, copies the Cargo documentation into `dist-site/docs/rust/`, then runs:

```sh
python3 tools/check-site.py dist-site --require-rustdoc
```

Canonical metadata, language alternates, `sitemap.xml`, `robots.txt` and `.nojekyll` are generated with the site. The default base URL is `https://craft-and-code.github.io/aede/`; pass `--base-url` when reviewing another deployment URL. The generated tree is the artifact to publish.

The signup form posts directly to the configured email relay. Visitor addresses are not stored in the public repository or browser storage. Changing a recipient or privacy statement requires updating the authored landing page and the corresponding Markdown information pages together. A local preview and an intercepted browser submission cannot confirm activation or delivery of the external email service; that is verified by its recipient confirmation flow.

FormSubmit activation is a recipient-side setup step. Initially, the form action contains the recipient address; that address remains readable in the public HTML until it is replaced. After the recipient activates the form, use FormSubmit’s “Invisible emails” identifier in the action (`https://formsubmit.co/<identifier>`) instead of the address. This identifier is intended for public form submissions; it is distinct from the private activation link and from an Email Link URL under `/el/`. Never publish the activation link or disguise a plaintext recipient address with client-side encoding. The visitor-facing signup stays a single email field and button. Contact and data requests use the maintainer-provided FormSubmit Email Link.
