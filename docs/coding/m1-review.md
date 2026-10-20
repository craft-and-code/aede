# M1 identification, enrichment and source safeguards

M1 keeps provider claims and provenance separate from local file facts. This is a domain summary of retained corrections, not a chronological pass log. See [sources](../sources.md), [roadmap](../design/roadmap.md), [manual verification](m1-manual-verification.md) and [current state](current-state.md).

## Trust, namespaces and identity

Accepted/identified source evidence is projected consistently across queries, detail pages, country/role selectors, canonical works and continuation links. All trusted observations participate before deduplication; a pending/rejected first observation cannot conceal a later trusted one. Review duplicates cannot make trust depend on input order. Accepted approximate evidence retains its original matched score/decision rather than being displayed as identified.

Edition MBIDs and release-group MBIDs are different namespaces. A no-group edition fallback is not an invented conflict with a known group. Source URLs select their real entity kind; an AcoustID observation storing a MusicBrainz recording MBID cannot become an AcoustID-track URL. Exact local references remain usable when an MBID alias is ambiguous; homonyms are not collapsed merely by name.

Do not enforce equality between a queried and returned MusicBrainz MBID: [merged identifiers may redirect to a surviving identifier](https://musicbrainz.org/doc/MusicBrainz_Identifier). A changed canonical ID alone is not a demonstrated unrelated response. Artist/recording source resolution, missing-album identity comparison and credited-person navigation retain this distinction.

Label search completion and Discogs biography-link lookups also permit a valid UUID redirect. Completing those relationships retains the original source selector, search confidence and explicit review; the returned canonical identifier does not replace the reviewed claim.

## Credits and personal corrections

Canonical recording/work and parent-work evidence reaches every exact supported placement, not only the file that supplied one observation. Role filters include trusted source contributions alongside local graph credits; distinct performers do not collapse under one display name. Uncertain membership dates remain explicitly counted rather than silently hidden or claimed as performance evidence.

Manual credits retain their selected recording/edition target through later identification or representative changes. Adding a new credit cannot move older claims to another edition, and changing an observation cannot duplicate the same assertion. Personal contexts/exclusions survive canonicalization into a local artist without widening to ambiguous identities. A direct recording correction never follows a later recording occupying the same file path.

## Provider admission and retry semantics

Malformed supplied arrays, nested targets, facts containers and completion markers are refused; unreadable answers are not successful empty results or permanent cached absences. This applies to MusicBrainz relationships, Wikidata portraits, Wikimedia prose and Fanart tables. Unicode dates are validated without unsafe byte slicing. Completion for one canonical recording is not restricted to only one placement.

Artist and label candidates must carry a valid MusicBrainz UUID before ranking, issuing an identifier lookup or replacing a stored claim, including during ordinary identification and label refreshes. Malformed candidates cannot hide a lower-ranked valid match; an unusable response retains the prior facts and reviews and remains retryable. A different valid UUID from a lookup can be a merged identity and is not refused solely for differing from the requested one.

Secondary fetches preserve accepted identification, source trust and exact scope. Existing artwork/lyrics is established from current confined regular files, not remembered catalogue hints alone. Removed sidecars become eligible again; published covers/logos/portraits do not trigger redundant alternate downloads while catalogue metadata catches up. LRC metadata alone does not establish readable words.

Missing-artwork cache identities cannot escape the cache. Native artwork rechecks initially anonymous access when authenticated mode becomes active. Phémios missing-cover requests reuse their authenticated session agent and exact TLS identity, just like ordinary artwork; neither default trust nor an insecure verifier is substituted.

Fingerprint backends describe the complete requested input: fallback fpcalc cannot silently stop at its default two-minute prefix while retaining full duration. Default identification honors explicit plural artist tags before deciding acoustic rescue is necessary; full/scoped acoustic requests remain explicit choices.

## CLI and retained exports

Human/machine windows apply consistently to source-only subsidiary lists and source-backed facets. Unsupported combinations, admitted-but-unused source options and conflicting fetch/logo operations are refused. Literal artist/title hints use the central shell quoting policy, while unstyled prose, source IDs, relations and notes use terminal escaping. Source and local names are never executable command fragments.

Export equivalence compares graph content independently of its legitimate `exported_at` timestamp. Test fixtures do not depend on two commands completing within one civil-clock second. Attribution, confidence and licensing remain intact in all source projections.

## Verification boundaries

Behavioral tests preserve offline source/import/review flows, canned malformed provider responses, exact source identity and public CLI results. The [manual procedure](m1-manual-verification.md) distinguishes live-source observations from fixtures. Provider availability, source correctness, licence compliance and changed upstream schemas still require operational attention; offline checks do not certify every live response. No browse should silently fetch enrichment or overwrite audio/tags. Detailed regressions and before/after evidence remain in tests/Git rather than repeated status paragraphs.
