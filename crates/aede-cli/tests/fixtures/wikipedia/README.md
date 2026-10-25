# Wikipedia summary language fixtures

Abbreviated public responses verified on 2026-10-25:

- <https://www.wikidata.org/wiki/Special:EntityData/Q11649.json>
- <https://fr.wikipedia.org/api/rest_v1/page/summary/Nirvana_(groupe)>
- <https://en.wikipedia.org/api/rest_v1/page/summary/Nirvana_(band)>

Only fields consumed by the summary pass are retained. Wikipedia extracts retain their original source URL and CC BY-SA 4.0 attribution in the tested `Prose`. Tests explicitly derive missing-link and empty-article variants from these fixtures to exercise negative cache outcomes; those mutations are not claims about the current Nirvana articles.
