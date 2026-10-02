<div id="browsing-navigating-your-collection" data-legacy-anchor></div>

# Parcourir et découvrir sa collection

<div id="the-art-of-the-facet-reading-the-liner-notes" data-legacy-anchor></div>

## Explorer une facette et lire les crédits

La bibliothèque relie fichiers, artistes, œuvres et contributions. Chaque entité dispose de sa propre page. Comme `artist`, `album` et `track`, les commandes `genre` et `label` permettent d’explorer un ensemble musical.

```sh
aede genre metal            # albums et musiciens qui y participent
aede label "Blue Note"      # parcourir les albums de ce label
```

Si le nom ne correspond pas exactement, Aède élargit la recherche et l’indique. `aede genre metal` rassemble ainsi Black Metal et Doom Metal. Chaque page représente une **sélection** exportable : `aede genre jazz --m3u` produit une playlist des pistes de jazz correspondantes.

Ces facettes servent aussi de filtres sur les albums :

```sh
aede albums --genre metal --year 1994
aede albums --label "Blue Note"
```

**Un rôle se consulte dans deux directions.** `--role` sert à trouver les personnes qui l’exercent ou les contributions d’une personne donnée :

```sh
aede artists --role producer                    # producteurs présents dans la bibliothèque
aede artist Ozzy --role performer               # pistes auxquelles Ozzy participe comme interprète
aede artist Ozzy --role performer --m3u         # exporter cette sélection en playlist
aede artists --role composer --csv --output=composers.csv
```

Sur la **liste des artistes**, la question est « qui exerce ce rôle dans ma collection ? ». Sur la **page d’une personne**, elle devient « à quoi cette personne a-t-elle contribué dans ce rôle ? ». Les crédits distincts permettent ces deux recherches, au-delà d’un simple nom d’artiste dans une colonne.

_Remarque :_ un rôle nécessite une personne à laquelle le rattacher. `aede album "<title>" --role performer` est donc refusé. Pour filtrer la liste d’albums par artiste, employer `aede albums --artist "<name>"` ; pour préciser une piste, `aede track "<title>" --artist "<name>"`. La page `album` individuelle n’accepte pas `--artist`.

Saisir le rôle tel qu’il est **affiché**, par exemple `--role "album artist"` ou `--role album`. Les guillemets sont facultatifs pour cette option de nom. Le vocabulaire affiché est aussi celui qu’Aède reconnaît dans les filtres.

<div id="your-unique-vocabulary" data-legacy-anchor></div>

### Le vocabulaire de votre bibliothèque

Les rôles dépendent des tags réellement présents. `aede stats` en affiche la liste et les nombres d’artistes et de crédits. Ce tableau aide à comprendre pourquoi un rôle recherché ne trouve rien dans les tags du catalogue.

```text
Roles

  Role      Artists  Credits
  ────────  ───────  ───────
  composer       48      412
  producer       11       87
```

_(Remarque : `main` et `album` ne figurent pas dans ce résumé. Le tableau Roles compte les crédits locaux du catalogue. Les crédits MusicBrainz peuvent déjà être récupérés avec `aede fetch --credits` et consultés dans la navigation et les requêtes avec leur attribution ; ils ne réécrivent ni les tags locaux ni ce compte de crédits locaux.)_

<div id="the-collectors-compass-whats-missing" data-legacy-anchor></div>

## Repérer les albums manquants

La page d’artiste peut aussi indiquer les albums absents de la bibliothèque, à partir de la discographie de source déjà récupérée :

```text
  3 studio albums MusicBrainz credits to them and this shelf does not hold: aede missing "Portishead"
```

Cette indication n’apparaît pas si rien ne manque dans la comparaison ou si la discographie de l’artiste n’a pas été demandée. La page reste centrée sur les informations disponibles et utiles.

<div id="mapping-your-collection-where-is-the-music-from" data-legacy-anchor></div>

## Situer l’origine des artistes

```sh
aede countries                        # vue des origines de la bibliothèque
aede artists --country france         # artistes dont l’origine correspond à France
aede artists --country united         # correspond à UK et US, avec indication
aede countries --csv --output=map.csv
```

**L’origine géographique ne vient pas des tags locaux.** `RELEASECOUNTRY` indique un pays lié à une édition, pas nécessairement celui de l’artiste. Une édition américaine d’un groupe français ne transforme pas ce groupe en artiste américain. Aède consulte les informations d’origine de MusicBrainz.

La sortie distingue les données connues, absentes et non encore demandées :

```text
Countries (4 in total)

  Country         Code  Also  Artists  Tracks  Duration      Size
  ──────────────  ────  ────  ───────  ──────  ────────  ────────
  France          FR              2       2      0:02   40.3 kB
  United Kingdom  GB    UK        1       1      0:01   20.1 kB
  United States   US              1       4      0:04   80.6 kB
  County Antrim                   1       1      0:01   20.1 kB
  1 place with no ISO code: MusicBrainz gave none, or the artists were fetched
  before Aède kept it — aede fetch --full asks again
  these are the areas MusicBrainz holds for your artists: usually a country,
  sometimes a county or a city
  7 artists not asked about yet: aede fetch
  2 artists asked about, with no area on record
```

**`Code` et `Also` désignent des informations différentes.** `Code` est le code ISO officiel. `Also` contient les initiales supplémentaires qu’Aède déduit du nom pour faciliter la recherche. L’absence d’initiales supplémentaires, par exemple pour Canada, ne signifie pas que la donnée est incomplète.

**`County Antrim` est une aire géographique valide.** MusicBrainz peut fournir un pays, un comté ou une ville. Aède conserve cette précision au lieu de transformer arbitrairement toutes les aires en pays.

Un **artiste jamais interrogé** se distingue d’un **artiste interrogé sans origine connue**. Le rapport indique les deux situations pour que vous sachiez ce qu’un nouvel enrichissement peut apporter.

<div id="smart-collision-free-abbreviations" data-legacy-anchor></div>

### Abréviations calculées sans collision

Les abréviations de `Also` dépendent des aires présentes dans votre bibliothèque. Aède reconnaît :

1. Le **nom exact** (`--country "united kingdom"`).
2. Le **code ISO officiel** (`--country gb`).
3. Les **initiales** (`--country uk`, `--country nz`).
4. Une **partie du nom** (`--country kingdom`).

Les listes de variantes usuelles ou traduites (`USA`, `Royaume-Uni`) ne sont pas des alias reconnus systématiquement. Employer le nom de source, le code ou les initiales affichées.

**Des initiales ambiguës sont retirées.** County Antrim donnerait `CA`, également code officiel du Canada. Si la bibliothèque contient des artistes des deux aires, les initiales `CA` sont retirées pour le comté au profit du code officiel du Canada. Sans artiste canadien, elles peuvent rester pour County Antrim. Les aires de la bibliothèque déterminent donc quelles abréviations sont utilisables sans confusion.

<div id="the-crate-digger-querying-albums-vs-tracks" data-legacy-anchor></div>

## Rechercher des albums ou des pistes

La recherche peut viser les pistes en détail ou regrouper les résultats par album. Pour obtenir une liste d’**albums** :

```sh
aede albums --query "album.rating:>=4"        # albums bien notés plutôt que seules pistes notées
aede albums --query "album.tag:vinyl"
aede albums --artist ozzy --query "album.rating:>=4"
```

Un album figure dans le résultat si **au moins une de ses pistes** répond aux critères. Les options et l’expression de requête se combinent pour restreindre la sélection.

La grammaire propose :

- **Intervalles :** `year:1990..` ou `duration:..3:30`.
- **Comparaisons :** `>`, `>=`, `<`, `<=`.
- **Correspondances :** `field:=value` pour une valeur exacte ; valeur simple pour une partie du texte.
- **Durées :** `3:45` ou nombre de secondes.
- **Logique :** `-` ou `NOT` pour exclure ; termes côte à côte pour AND.

Un tag manquant reste **absent** : il n’est pas transformé en zéro. Un fichier sans année ne devient donc pas arbitrairement un enregistrement antérieur à 1970.

Les sélections peuvent être exportées avec `--csv`, `--json` ou `--m3u`. Une requête enregistrée devient une collection dynamique, réévaluée sur la bibliothèque actuelle et exportable en playlist.
