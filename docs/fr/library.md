<div id="building-the-library-archival-management--vault-structure" data-legacy-anchor></div>

# Construire la bibliothèque : gestion des fichiers et structure

<div id="tagging-metadata-integrity-with-musicbrainz-picard" data-legacy-anchor></div>

## Tags : préparer les métadonnées avec MusicBrainz Picard

Aède respecte une frontière stricte : **il n’écrit jamais dans vos fichiers audio originaux**. Tags, noms de fichiers et organisation des dossiers restent intacts. Cela protège les originaux, mais demande de préparer les métadonnées initiales avec des outils d’écriture de tags dédiés.

[MusicBrainz Picard](https://picard.musicbrainz.org/) est le compagnon recommandé pour cela. Les outils se complètent : Picard examine l’audio, interroge MusicBrainz et écrit des tags normalisés `MUSICBRAINZ_*` ; Aède les lit, indexe la structure et construit le catalogue interrogeable.

Avec des fichiers préparés par Picard, Aède lit les identifiants MusicBrainz pour établir les identités locales. Un `aede fetch --credits` explicite peut ensuite obtenir relations/crédits attribués de MusicBrainz ; le scan seul n’effectue jamais cette requête réseau. Les identifiants communs réduisent l’ambiguïté sans garantir l’accord entre tags et informations externes. Sans Picard, Aède lit les tags standards ID3, Vorbis ou MP4 et doctor indique les champs manquants/incomplets.

<div id="resolving-identity-when-one-artist-appears-twice" data-legacy-anchor></div>

## Identité : lorsqu’un artiste apparaît deux fois

Les fichiers préparés avec Picard portent les identifiants uniques `MUSICBRAINZ_ARTISTID`. Au scan, Aède réunit les variantes d’écriture — `Ozzy Osbourne` et `O. Osbourne`, par exemple — sous une entrée d’artiste, sans deviner par ressemblance. Le détail d’artiste liste explicitement les alias réunis.

Sans tags ou identifiants MusicBrainz, Aède évite les rapprochements risqués de noms partiels, qui pourraient fusionner Angus Young et Neil Young. Doctor signale plutôt les paires suspectes à examiner, puis `aede merge` permet une réunion manuelle. Aucun des deux ne modifie les fichiers audio.

<div id="folder-exclusions-guarding-the-vault-boundaries" data-legacy-anchor></div>

## Exclusions de dossiers : choisir les limites de la bibliothèque

Un dossier audio peut contenir livres audio, podcasts, dossiers d’arrivée `_incoming` ou banques d’échantillons sans place dans votre bibliothèque musicale. Aède permet d’exclure des chemins sans vous imposer de réorganiser les fichiers :

```
aede roots --exclude ~/Music/Audiobooks     # permanently ignore this directory
aede roots                                  # display all watched and excluded roots
aede roots --exclude ~/Music/Audiobooks --remove
```

Les exclusions sont enregistrées dans le catalogue avec les racines suivies et survivent aux rescans complets. **Un scan ne doit pas détruire des données qu’il ne peut recalculer** ; les exclusions sont des choix explicites de l’utilisateur.

Les chemins sont résolus de manière canonique : les liens symboliques vers un dossier exclu sont correctement reconnus.

Les changements d’exclusion s’appliquent par rescan automatique des dossiers suivis avant la fin de la commande :

- Ajouter/retirer une exclusion réindexe automatiquement les chemins concernés.
- `--no-scan` reporte l’indexation pour regrouper des changements sur un stockage lent.
- `aede reset` retire le catalogue, racines suivies et exclusions incluses ; conclusions indépendantes, données personnelles et sources restent intactes.

<div id="disc-anatomy-box-sets--multi-disc-releases" data-legacy-anchor></div>

## Organisation des disques : coffrets et albums multidisques

Coffrets et éditions spéciales sont souvent rangés en sous-dossiers de disques :

```
Nobuo Uematsu/1997 FINAL FANTASY VII [FLAC]/Disc 1/
Nobuo Uematsu/1997 FINAL FANTASY VII [FLAC]/Disc 2/
```

Les dossiers d’albums principaux distinguent les masterings/éditions ; `Disc 1`, `CD2` ou `Disque 3` représentent des subdivisions physiques d’une même sortie. Aède les rassemble automatiquement dans l’album parent.

Les pistes portent la numérotation habituelle `1-01`, `2-07`. Cette notation s’affiche seulement pour les albums multidisques afin de garder les autres listes simples. Les numéros viennent des tags `DISCNUMBER`, ou des dossiers si les tags manquent.

Les résumés donnent le nombre de disques réellement présents :

```
  4 discs · 85 tracks · 4:34:11 · 1.5 GB
```

Si le dernier disque d’un coffret de quatre manque sur le stockage, Aède affiche `3 discs` : état des fichiers présents, pas total théorique des tags.

<div id="managing-compilations" data-legacy-anchor></div>

## Gérer les compilations

Le scan marque une sortie comme compilation lorsque le tag `COMPILATION` vaut `1`, `true` ou `yes`, ou lorsque son artiste d’album est un libellé reconnu tel que `Various Artists`, `VA` ou `Artistes divers`. Ces sorties sont retirées des discographies individuelles pour éviter de les encombrer.

Des artistes différents selon les pistes ne suffisent pas à reconnaître une compilation. Sans artiste d’album ni tag de compilation, les fichiers peuvent former des sorties locales séparées selon leurs artistes. Renseignez ces tags de manière cohérente avant de scanner une compilation.

Vous pouvez les examiner avec des options dédiées :

```
aede albums --compilations       # sorties marquées comme compilations
aede albums --no-compilations    # autres sorties
```

Fournir les deux options est refusé comme contradiction.

<div id="interpreting-the-scan-report" data-legacy-anchor></div>

## Lire le rapport de scan

Chaque `aede scan` se termine par un bilan détaillé :

| Mesure | Description |
| :--- | :--- |
| **Fichiers trouvés (`Files found`)** | Fichiers audio découverts pendant le parcours, doublons de chemins retirés. |
| **Lus sur disque (`Read from disk`)** | Fichiers demandant une nouvelle lecture des métadonnées, y compris les tentatives signalant ensuite une erreur. |
| **Réutilisés (`Reused from previous scan`)** | Chemin, taille et date inchangés ; métadonnées reprises directement du catalogue. |
| **Disparus (`Gone since last scan`)** | Anciens fichiers absents du stockage, retirés proprement du catalogue. |
| **Analyses importées (`Analyses imported`)** | [Rapports FlacCompagnon](imported-analyses.md#what-another-tool-found) détectés et importés. |
| **Analyses rattachées (`Analyses now attached`)** | Analyses en attente liées avec succès aux fichiers nouvellement scannés. |
| **Durée (`Elapsed`)** | Temps réel du parcours et de l’ingestion des métadonnées. |

`Files found` compte les chemins audio découverts ; les entrées inaccessibles conservées sont indiquées séparément. `Read from disk` et `Reused from previous scan` décrivent le travail de métadonnées sur les fichiers découverts. Les erreurs de lecture sont signalées sous le bilan. Les anciennes entrées des chemins inaccessibles sont conservées, même lors d’un scan complet, et comptées séparément ; un dossier illisible n’est pas considéré comme vide.

<div id="vault-location-storage-footprint--scaling" data-legacy-anchor></div>

## Emplacement, taille des données et grandes bibliothèques

Métadonnées et état sont enregistrés dans un dossier de données commun :

```
aede --data=/volume1/aede stats     # override catalog path for a single command
export AEDE_HOME=/volume1/aede      # globally relocate catalog storage
```

`aede stats` indique emplacement et volume du catalogue :

```
This catalog

  Kept in       /Users/kcell/.local/share/aede
  Weighs        11.2 MB
  Last scanned  3 days ago
  aede backup writes all four stores to one file; AEDE_HOME moves them
```

Sans AEDE_HOME, Aède utilise `$XDG_DATA_HOME/aede` ou `~/.local/share/aede`.

Les requêtes utilisent un document JSON en mémoire. Ces mesures sur bibliothèques synthétiques (12 pistes par album) précèdent la séparation de `conclusions.json` ; ces chiffres historiques ne constituent pas un budget actuel. Une nouvelle référence synthétique figure dans les [mesures M2](../coding/m2-storage-benchmark.md) :

| Pistes | Taille de `catalog.json` | Sauvegarde | Chargement | Mémoire maximale |
| :--- | :--- | :--- | :--- | :--- |
| **10 000** | 12.4 MB | 0.79 s | 0.41 s | 181 MB |
| **50 000** | 62.5 MB | 3.88 s | 2.17 s | 897 MB |
| **200 000** | 252.0 MB | 16.37 s | 13.42 s | 3,586 MB |

Ces résultats historiques montraient un chargement d’environ deux secondes jusqu’à 50 000 pistes, avec moins de 1 Go de RAM. Au-delà de 100 000, la mémoire augmente proportionnellement. Les évolutions envisagées pour de grandes collections figurent dans [Architecture](../design/architecture.md#when-this-becomes-a-database).

<div id="safeguarding-your-data-backups--disaster-recovery" data-legacy-anchor></div>

## Protéger les données : sauvegarde et récupération

Aède rassemble ses stores JSON dans une sauvegarde portable versionnée. Elle ne contient ni musique originale ni images/paroles dérivées, à sauvegarder séparément :

```
aede backup ~/aede-2026-09-03.json    # export catalog state and annotations
aede restore ~/aede-2026-09-03.json   # restore system state from backup
```

La sauvegarde conserve quatre stores :

1. **Catalogue (`catalog.json`)** : métadonnées scannées, graphe dérivé, dossiers suivis et exclusions.
2. **Conclusions (`conclusions.json`)** : verdicts d’intégrité, empreintes acoustiques et analyses importées.
3. **Utilisateur (`user.json`)** : notes, étoiles, comptes d’écoute, collections, fusions manuelles et éléments ignorés, qui ne se reconstruisent pas d’après les fichiers audio.
4. **Sources (`sources.json`)** : métadonnées obtenues auprès des services externes, dont MusicBrainz.

```
Backup

  catalog            20 148 tracks, 1 604 albums
  conclusions        18 412 file results, 236 analyses
  what you said      312 annotations, 4 collections, 9 records set aside
  what sources said  1 841 records
→ /Users/kcell/aede-2026-09-03.json (9.7 MB)
```

Chaque store possède sa version de format dans le bundle. La restauration les examine indépendamment. Si un store manque, par exemple dans une sauvegarde faite avant tout téléchargement externe, son fichier local existant reste intact au lieu d’être écrasé/supprimé.

Avant de restaurer, Aède résume l’opération et demande confirmation :

```
Restore

  made 3 days ago by Aède 0.3.0
  into /Users/kcell/.local/share/aede
  catalog            20 148 tracks, 1 604 albums — replaces what is there
  what you said      312 annotations — replaces what is there
  what sources said  not in this backup — left as it is
```

Si une racine restaurée est démontée/inaccessible, Aède avertit explicitement avant restauration pour éviter une réduction accidentelle du catalogue au scan suivant. En mode non interactif, `--yes` accepte les confirmations.

<div id="catalog-management--resetting-state" data-legacy-anchor></div>

## Gérer et réinitialiser le catalogue

`aede roots` résume les dossiers suivis :

```
Watched folders

  Folder                 Tracks  Duration       Size
  ─────────────────────  ──────  ────────  ─────────
  /Volumes/Music/FLAC     18 402   52 days     4.1 TB
  /Users/kcell/Music         746   2 days    112.4 GB
  (no longer watched)         92   6 h        8.1 GB
```

Pour retirer le catalogue sans toucher aux fichiers audio, utilisez `aede reset` :

```
$ aede reset

About to remove the catalog

  Tracks                  20 148
  Watched folders              2
  Integrity verdicts      20 148
  Imported analyses          312
  File                    9.4 MB
  a scan rebuilds the catalog; watched folders must be named again
  integrity verdicts, fingerprints and imported analyses are kept
  Type "yes" to confirm:
```

`--yes` contourne la confirmation interactive pour les scripts automatisés.

<div id="testing-with-synthetic-datasets" data-legacy-anchor></div>

## Essayer avec des données synthétiques

Le générateur de la suite de tests permet d’essayer la bibliothèque :

```
tools/demo-library.sh /tmp/demo-music   # generates synthetic test audio (requires ffmpeg)
aede scan /tmp/demo-music
aede doctor
```

L’environnement contient volontairement tags manquants, doublons, pistes absentes et codecs mélangés : un terrain d’essai pour doctor et la gestion du catalogue.

## Prévoir les changements

`aede scan --dry-run` décrit les fichiers ajoutés, modifiés, disparus et illisibles sans publier catalogue, racines, conclusions ou données personnelles. --json fournit tous les chemins et compteurs sans mélanger la progression au résultat. La préparation peut lire les métadonnées et rapports locaux. Un sous-dossier illisible conserve les anciennes entrées, y compris avec --full ; il ne devient pas un dossier vide. Les fractions de seconde sont conservées si le système les fournit ; les anciennes lignes sans cette précision sont relues une fois.

Les rapports acoustiques importés gardent l’identité et la précision temporelle enregistrées par leur source, actuellement à la seconde. La précision du scan protège la réutilisation des métadonnées et le rattachement des verdicts/empreintes internes ; elle ne prouve pas la fraîcheur à la fraction de seconde d’un rapport externe qui ne l’a jamais enregistrée.
