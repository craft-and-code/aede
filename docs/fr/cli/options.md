# Options, syntaxe et sorties

Une option change le comportement d’une commande. Aède distingue options réellement partagées, familles réutilisées à portée limitée et réglages spécialisés. Ne pas supposer qu’une option connue fonctionne partout : le tableau de chaque commande fait référence.

## Présentation et données partagées

| Option | Sens |
| --- | --- |
| `--data DOSSIER` | Dossier de données Aède, pas dossier musical à scanner. Concerne les opérations utilisant les données mémorisées. |
| `--no-color` | Désactiver les couleurs ANSI du terminal. |
| `--help`, `-h` | Montrer index ou aide de commande et s’arrêter. |
| `--version`, `-v`, `-V` | Afficher la version et s’arrêter. |

Ordre des données : `--data`, `AEDE_HOME`, `$XDG_DATA_HOME/aede`, `~/.local/share/aede`, puis `.aede` du dossier courant si HOME est absent. L’inspection directe n’exige pas de catalogue ; la lecture utilise encore les données pour sélections/historique. Garder le même emplacement avec le serveur.

## Saisir les valeurs

```sh
aede albums --limit 10
aede albums --limit=10
aede track "So What" --artist "Miles Davis"
aede play "/path/to/audio.flac" --bass -2
aede note album "Kind of Blue" --file -
aede file -- ./-track.flac
```

`--option valeur` et `--option=valeur` fonctionnent. Chemins/nombres/mots-clés prennent un élément. Les options de noms (`--artist`, `--album`, `--with`, `--genre`, `--label`, `--comment`, `--role`, `--country`, `--text`, `--from`, `--tag`, `--query`, `--collection`, `--instrument`) absorbent les mots jusqu’à la prochaine option : placer le nom positionnel d’abord. Citer chemins et noms composés. `--` arrête la lecture d’options pour un chemin commençant par tiret. `-` seul lit l’entrée standard des notes. Graves/aigus acceptent spécialement les nombres négatifs (`--bass -2` ou `--bass=-2`).

Raccourcis séparés : `-j` signifie `--json` de portée limitée ; `-o FICHIER` signifie `--output`. `-o=FICHIER` fonctionne. Ne pas combiner les lettres (`-jo` n’est pas déclaré). Options inconnues/valeurs manquantes sont vérifiées avant aide/version. Répéter une option garde actuellement sa dernière valeur ; l’écrire une fois évite un remplacement involontaire.

## Familles réutilisées

Pagination : `--limit N`, `--offset N`, `--all` seulement sur les commandes listées ci-dessous. N entier ; limite positive, décalage depuis zéro ; all et limit incompatibles. Une liste paginée demande généralement 50 lignes, certains rapports moins pour leurs listes secondaires. L’export garde les lignes filtrées/paginées ; demander tout volontairement.

Sortie : choisir CSV, JSON ou M3U si proposé. `--output`/`-o` écrit un véritable export, pas toute page humaine. `--separator` exige CSV ; `export --tracks` exige CSV. CSV cite les titres contenant des virgules : un outil qui coupe aveuglément à chaque virgule casse ces colonnes. `--separator tab` convient aux traitements simples à tabulations. M3U garde des chemins, sans copier l’audio.

Confirmation : `--yes` existe seulement pour reset, history, fetch, backup, restore. Il accepte leur confirmation réelle, sans ignorer les erreurs. Sans terminal, une confirmation nécessaire refuse sauf acceptation volontaire.

Réutilisation : `--full` concerne scan/check/spectrum/fetch/fingerprint avec des effets différents ; analyze utilise `--force`. Ne pas déduire l’effet du nom seul.

## Matrice complète des options reconnues

Cette liste suit répartition et traitements actuels. Elle décrit la portée acceptée, pas toutes les combinaisons valides. Contraintes sur chaque page. Limites actuelles : `missing --source` et `merge --source` acceptés sans appliquer le filtre ; `sources --source` filtre list/forget et choisit la provenance de template, mais résumé/export incluent tout. `--compress`/`--quality` relèvent de copy même si la répartition actuelle ne refuse pas ces noms sur toutes les autres commandes. Employer seulement leur portée documentée.


| Option | Commandes / portée |
| --- | --- |
| `--data` | Partagée (voir ci-dessus) |
| `--port N` | [serve](serve.md) |
| `--replace` | [scan](scan.md), [copy](copy.md) |
| `--remove` | [roots](roots.md), [relation](relation.md), [missing](missing.md), [collection](collection.md), [love](love.md), [rate](rate.md), [note](note.md), [tag](tag.md), [played](played.md), [history](history.md) |
| `--limit N` | [stats](stats.md), [doctor](doctor.md), [credits](credits.md), [import](import.md), [review](review.md), [relations](relations.md), [missing](missing.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [history](history.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md) |
| `--sort ORDER` | [query](query.md), [collection](collection.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [labels](labels.md), [years](years.md) |
| `--severity error\|warning\|info` | [doctor](doctor.md) |
| `--artist NAME` | [credit](credit.md), [albums](albums.md), [track](track.md) |
| `--album TITLE` | [track](track.md) |
| `--with NAME` | [artist](artist.md) |
| `--separator ";" / --separator tab` | [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [years](years.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md), [export](export.md) |
| `--csv` | [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [years](years.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md), [export](export.md) |
| `--tracks` | [export](export.md) |
| `--m3u` | [query](query.md), [collection](collection.md), [genre](genre.md), [label](label.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md) |
| `--year YYYY` | [albums](albums.md) |
| `--output FILE / -o FILE` | [sources](sources.md), [rules](rules.md), [relations](relations.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [years](years.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md), [export](export.md) |
| `--threads N` | [scan](scan.md), [analyze](analyze.md), [check](check.md), [copy](copy.md), [spectrum](spectrum.md) |
| `--force` | [analyze](analyze.md) |
| `--show-results` | [analyze](analyze.md) |
| `--json-layout album\|artist` | [analyze](analyze.md) |
| `--genre NAME` | [albums](albums.md) |
| `--label NAME` | [albums](albums.md) |
| `--json / -j` | [analyze](analyze.md), [stats](stats.md), [doctor](doctor.md), [credits](credits.md), [relations](relations.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [years](years.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md), [export](export.md) |
| `--no-color` | Partagée (voir ci-dessus) |
| `--yes` | [reset](reset.md), [backup](backup.md), [restore](restore.md), [fetch](fetch.md), [history](history.md) |
| `--forget` | [import](import.md), [sources](sources.md), [missing](missing.md), [merge](merge.md) |
| `--pending` | [import](import.md) |
| `--list` | [import](import.md), [sources](sources.md), [missing](missing.md), [merge](merge.md), [fingerprint](fingerprint.md) |
| `--members` | [artist](artist.md) |
| `--no-scan` | [roots](roots.md) |
| `--lyrics` | [fetch](fetch.md), [track](track.md), [search](search.md) |
| `--simple` | [playlist](playlist.md) |
| `--artists` | [playlist](playlist.md) |
| `--extras none\|cover\|images\|all` | [copy](copy.md) |
| `--dry-run` | [copy](copy.md), [spectrum](spectrum.md), [playlist](playlist.md), [fetch](fetch.md), [extract](extract.md), [fingerprint](fingerprint.md) |
| `--verify` | [copy](copy.md) |
| `--safe-names` | [copy](copy.md) |
| `--raw-names` | [copy](copy.md) |
| `--collection NAME` | [copy](copy.md) |
| `--compress FORMAT` | [copy](copy.md) |
| `--quality SETTING` | [copy](copy.md) |
| `--source NAME` | [import](import.md), [sources](sources.md), [review](review.md), [relations](relations.md), [missing](missing.md), [merge](merge.md) |
| `--compilations` | [albums](albums.md) |
| `--no-compilations` | [albums](albums.md) |
| `--role ROLE` | [credit](credit.md), [artists](artists.md), [artist](artist.md) |
| `--comment TEXT` | [albums](albums.md), [track](track.md) |
| `--comments` | [search](search.md) |
| `--notes` | [search](search.md) |
| `--offset N` | [stats](stats.md), [doctor](doctor.md), [credits](credits.md), [import](import.md), [review](review.md), [relations](relations.md), [missing](missing.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [history](history.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md) |
| `--all` | [stats](stats.md), [doctor](doctor.md), [credits](credits.md), [import](import.md), [review](review.md), [relations](relations.md), [missing](missing.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [history](history.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md) |
| `--help` | Partagée (voir ci-dessus) |
| `--version` | Partagée (voir ci-dessus) |
| `--full` | [scan](scan.md), [check](check.md), [spectrum](spectrum.md), [fetch](fetch.md), [fingerprint](fingerprint.md) |
| `--follow-symlinks` | [scan](scan.md) |
| `--include-hidden` | [scan](scan.md) |
| `--exclude FOLDER_OR_ID` | [roots](roots.md), [credit](credit.md) |
| `--stars N` | [rate](rate.md) |
| `--text TEXT` | [relation](relation.md), [note](note.md) |
| `--from KIND:NAME` | [note](note.md) |
| `--tag LABEL[,LABEL]` | [relations](relations.md), [relation](relation.md), [notes](notes.md) |
| `--file FILE_OR_-` | [note](note.md) |
| `--append` | [note](note.md) |
| `--query EXPRESSION` | [copy](copy.md), [collection](collection.md), [artists](artists.md), [albums](albums.md) |
| `--export` | [sources](sources.md), [rules](rules.md), [notes](notes.md) |
| `--import FILE` | [sources](sources.md), [rules](rules.md), [notes](notes.md) |
| `--template` | [sources](sources.md) |
| `--summaries` | [fetch](fetch.md) |
| `--discography` | [fetch](fetch.md) |
| `--covers` | [fetch](fetch.md) |
| `--size VALUE` | [spectrum](spectrum.md), [fetch](fetch.md) |
| `--images` | [fetch](fetch.md), [extract](extract.md) |
| `--country NAME` | [artists](artists.md) |
| `--identify` | [fetch](fetch.md) |
| `--credits` | [fetch](fetch.md) |
| `--add` | [credit](credit.md) |
| `--artist-id MBID` | [credit](credit.md) |
| `--instrument NAME` | [credit](credit.md) |
| `--recordings` | [fetch](fetch.md) |
| `--lang CODE` | [fetch](fetch.md) |
| `--portraits` | [fetch](fetch.md) |
| `--logos` | [fetch](fetch.md) |
| `--fanart` | [fetch](fetch.md) |
| `--no-logo` | [fetch](fetch.md) |
| `--no-label-logo` | [fetch](fetch.md) |
| `--no-portrait` | [fetch](fetch.md) |
| `--no-background` | [fetch](fetch.md) |
| `--no-banner` | [fetch](fetch.md) |
| `--no-album-cover` | [fetch](fetch.md) |
| `--no-cdart` | [fetch](fetch.md) |
| `--banners` | [fetch](fetch.md) |
| `--labels` | [fetch](fetch.md) |
| `--online` | [label](label.md) |
| `--offline` | [label](label.md) |
| `--accept ID` | [review](review.md) |
| `--reject ID` | [review](review.md) |
| `--undo ID` | [credit](credit.md), [review](review.md) |
| `--interactive` | [review](review.md) |
| `--graph` | [export](export.md) |
| `--normalize off\|track\|album` | [play](play.md) |
| `--bass DB` | [play](play.md) |
| `--treble DB` | [play](play.md) |
