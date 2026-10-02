
<div id="what-another-tool-found" data-legacy-anchor></div>

# Ce qu’un autre outil a mesuré

Cette fonction est facultative : la bibliothèque fonctionne aussi sans analyse acoustique.

Les lecteurs d’Aède lisent les tags, trames et conteneurs. Pour examiner l’audio décodé — indices de transcodage ou de suréchantillonnage, coupure spectrale, niveau sonore et MD5 audio FLAC — `aede analyze` appelle directement la bibliothèque Rust [FlacCompagnon](https://github.com/craft-and-code/FlacCompagnon). Les mesures restent attribuées à cet outil ; elles ne deviennent pas des observations des lecteurs de tags d’Aède.

```sh
aede analyze                            # analyser les albums catalogués et garder les mesures
aede analyze ~/Music/Album --json       # enregistrer aussi Album.json dans le dossier d’album
aede analyze ~/Music --json --threads 4 # un rapport par dossier d’album
```

Le dossier d’album vient du catalogue. Un album multidisque peut donc employer son dossier commun. Les fichiers sans tag d’album sont regroupés par dossier contenant l’audio. Les originaux et leurs tags restent en lecture seule ; les options de rapport écrivent des fichiers JSON distincts.

<div id="re-running-an-analysis-and-keeping-its-results" data-legacy-anchor></div>

### Relancer une analyse et conserver ses résultats

Les rapports valides de pistes inchangées sont réutilisés. `--force` recalcule l’audio sélectionné et autorise le remplacement des destinations de rapport sélectionnées. Sans option d’écriture, les mesures restent enregistrées dans Aède. `--json` écrit dans les dossiers d’album ; `--json-layout album|artist` active l’écriture et choisit les dossiers d’album ou leurs dossiers d’artiste parents. Une destination existante mal formée est refusée sans remplacement délibéré par `--force`. La référence [analyze](cli/analyze.md) détaille ces règles et toutes les options.

Les deux dispositions de FlacCompagnon sont compatibles :

| Rapport | Contenu | Comment le rattacher |
| --- | --- | --- |
| `Artist/Artist.json` | Plusieurs albums sous le dossier d’artiste | Scanner ce dossier ou un de ses parents, ou importer explicitement le rapport |
| `Artist/Album/Album.json` | Un album, y compris ses sous-dossiers de disques | Scanner le dossier d’album ou un de ses parents, ou importer explicitement le rapport |

Le rattachement suit le chemin, la taille et la date de modification de chaque fichier audio, indépendamment de l’emplacement du rapport. Scanner un album seul ne recherche pas de rapport dans son dossier d’artiste parent. Si celui-ci est hors du périmètre suivi, employer `aede import Artist/Artist.json`.

Pour un même fichier et une même source, des rapports qui se recouvrent donnent un seul résultat. Entre rapports décrivant le fichier actuel, la **date de modification du fichier de rapport** la plus récente gagne lorsque les dates sont disponibles. Le nom du rapport et sa disposition album/artiste ne lui donnent aucune priorité. Les mesures effectuées directement emploient leur date de fin. Ces dates sont conservées avec leur précision inférieure à la seconde dans `conclusions.json` : un ancien rapport ne remplace donc pas une mesure plus récente lors d’un scan suivant. Une mesure périmée d’une ancienne version du fichier ne bloque pas un rapport valide du fichier actuel. Dates égales ou inconnues : l’ordre d’import existant s’applique ; les anciens magasins restent lisibles.

Supprimer un rapport JSON **après son import ou son analyse** n’efface pas ses mesures, même après un scan complet. Elles restent dans `conclusions.json` jusqu’à un oubli explicite avec `aede import --forget` ou au remplacement par un résultat plus récent. Modifier taille ou date de l’audio peut les rendre obsolètes ; supprimer le rapport ne le fait pas. Un rapport qu’Aède n’a jamais lu ne peut pas être récupéré après sa suppression.

Un rapport produit par l’application FlacCompagnon ou sa commande autonome s’importe aussi :

```sh
aede import ~/Desktop/danzig-report.json
aede import ~/Desktop/reports/            # tous les .json, sous-dossiers compris
aede import --list                        # mesures conservées et états
aede import --pending                     # mesures sans fichier correspondant au catalogue
aede import --forget                      # retirer toutes les analyses conservées
aede import --forget --source=flaccompagnon
aede import --forget --pending            # retirer seulement les mesures en attente
aede import --forget --pending "/Volumes/OldDrive" # seulement sous ce dossier
```

L’import d’un dossier parcourt ses sous-dossiers récursivement. `--forget` efface les mesures d’Aède, sans supprimer ni rapport JSON d’origine ni musique.

<div id="the-order-does-not-matter" data-legacy-anchor></div>

## L’ordre des opérations est libre

Une mesure est d’abord rangée sous le chemin décrit par le rapport. Elle peut être conservée avant l’apparition d’un fichier correspondant dans le catalogue.

- **Importer avant le scan.** Les mesures sont gardées avec l’état `Waiting for a scan`. Un scan des fichiers correspondants les rattache automatiquement et annonce `Analyses now attached`. `doctor` compte celles qui attendent encore.
- **Scanner avant l’import.** Le chemin rattache directement le rapport. Si le rapport contient `file_md5`, il peut retrouver un fichier déplacé ou renommé par ses octets complets, même si sa date a changé. Un ancien rapport sans ce champ utilise un nom et une taille uniques, puis vérifie la date de modification.
- **Laisser le rapport près de l’album.** Le scan reconnaît les JSON FlacCompagnon et résume leur lecture. Il examine seulement le début des fichiers JSON pour reconnaître leur format ; les autres fichiers annexes ne deviennent pas des pistes audio.

Les dossiers suivis sont enregistrés sous leur chemin canonique. Un rapport décrivant un lien symbolique, ou `/var` là où macOS emploie `/private/var`, peut ainsi identifier le fichier réel. `file_md5` couvre tout le fichier, tags et images inclus : ce n’est pas le MD5 de l’audio décodé FLAC. Si deux copies cataloguées ont exactement les mêmes octets, Aède garde le résultat en attente plutôt que de choisir arbitrairement l’une d’elles.

<div id="when-a-scan-does-not-make-it-go-away" data-legacy-anchor></div>

## Quand un scan ne suffit pas au rattachement

Une mesure se rattache par son chemin ou par un fichier catalogué unique de même taille et même MD5 complet. Pour les anciens rapports sans `file_md5`, un nom et une taille uniques peuvent correspondre si la date de modification concorde.

Elle reste en attente si aucun candidat ne correspond ou si plusieurs copies identiques rendent le choix ambigu. Scanner un dossier déplacé peut résoudre le problème. Sans MD5 complet, un rapport ne retrouve pas nécessairement une piste renommée dont l’ancien nom a disparu.

`doctor` affiche un nombre de mesures en attente. Pour connaître les dossiers concernés, utiliser `aede import --pending`. La sortie est regroupée par dossier, avec nombre de mesures et source. Les libellés du terminal restent ceux du programme :

```text
$ aede import --pending

Waiting for a scan

  Folder                                                        Analyses  Source
  /Volumes/Musique externe/Bibliotheque/Danzig/1994 Danzig 4           2  flaccompagnon
  /Volumes/Musique externe/Bibliotheque/Ozzy Osbourne/1980 Blizzard…   4  flaccompagnon
  6 waiting analyses in all
  scan a folder to attach its analyses, or drop one that is gone for good:
  aede import --forget --pending <folder>
```

Ces chemins permettent de distinguer un disque simplement débranché d’un dossier définitivement supprimé. Le nom entre chevrons est une valeur à remplacer, pas une commande à recopier telle quelle. Si le dossier a réellement disparu, retirer seulement ses mesures en attente :

```sh
aede import --forget --pending "/Volumes/OldDrive/Music"
aede import --forget --pending
```

La première commande limite la suppression à ce dossier ; la seconde vise toute l’attente. Les mesures déjà rattachées restent conservées. `--pending` et `--forget --pending` acceptent des dossiers ; `--source` limite à un outil. Un dossier donné à `--forget` seul est refusé, car cette forme ne filtre pas par dossier.

Un rapport qui parle d’un fichier ne décrit pas forcément sa version actuelle. Les rattachements par chemin ou nom/taille vérifient aussi taille et date. Après déplacement, un MD5 complet identique prouve l’identité des octets ; un changement de date seul ne rend alors pas la mesure obsolète.

`aede track "<titre>"` montre un panneau attribué à l’outil, par exemple :

```text
Analysed by flaccompagnon

  FLAC audio MD5       Match
  File MD5             0123456789abcdef0123456789abcdef
  File CRC32           89abcdef
  Real bit depth       16 bits
  Cutoff               22.1 kHz
  Dynamic range        9.3 dB
  Integrated loudness  -14.2 LUFS
  Loudness range       6.0 LU
  True peak            0.28 dBTP
  Clicks               18 (first at 2.500 s)
  Click 1              at 2.500 s, channel 1, duration 0.000159 s
```

La page de piste expose niveau sonore, dynamique, phase, équilibre stéréo, indices de profondeur de bits, inférences de source, nombres de discontinuités, événements localisés conservés et empreintes disponibles. `FLAC audio MD5` est le résultat de comparaison de l’audio décodé ; `File MD5` et `File CRC32` couvrent le fichier complet. Le rapport peut conserver moins de positions d’événements que son nombre total d’événements détectés.

`track --json` change le format d’affichage et conserve l’entrée attribuée complète dans `analyses[].source_data`, y compris les champs ajoutés plus tard dans le format de rapport compatible. Le détail HTTP local `/api/v1/track` expose les mêmes données ; Aède garde cette entrée dans `conclusions.json`. Réimporter un ancien rapport ne recrée pas des champs absents de ce rapport. Relancer `analyze` ou importer un rapport plus récent apporte les mesures qu’une ancienne version n’avait pas conservées.

Trois règles aident à lire ces résultats :

**Les mesures ne remplacent pas silencieusement les observations d’Aède.** Une profondeur de bits lue dans une trame et une profondeur inférée par analyse spectrale ne proviennent pas de la même méthode. Leur désaccord doit rester visible avec ses sources.

**Les mesures cessent de décrire un fichier modifié.** À son chemin initial, taille et date sont vérifiées. Modifier ses tags peut rendre le panneau obsolète ; après un déplacement, modifier tags ou images change aussi le MD5 complet. Les mesures périmées refusées à l’import sont comptées et leurs dossiers sont affichés. Relancer l’analyse sur ces dossiers apporte des mesures actuelles.

**Un désaccord est un résultat à examiner.** `doctor` signale un MD5 audio divergent comme erreur, même si les CRC de `check` sont valides. Les CRC interrogent les trames/conteneurs ; le MD5 décodé compare l’audio à la signature conservée dans le FLAC. Un réencodage gardant une ancienne signature peut expliquer ce cas, mais la divergence ne prouve ni cette histoire précise ni une comparaison avec une copie originale séparée.

Les inférences « transcodé », « profondeur augmentée » ou « suréchantillonné » restent des verdicts attribués sur la page de piste, pas des erreurs d’Aède. Une coupure spectrale peut aussi refléter le contenu d’un enregistrement ancien. Lire les mesures et le contexte du master avant d’interpréter ces indices.

<div id="seeing-what-is-held" data-legacy-anchor></div>

## Voir toutes les mesures conservées

`--pending` montre ce qui attend ; `--list` montre aussi les mesures rattachées et celles devenues obsolètes :

```text
$ aede import --list

Imported analyses

  Folder                                          Analyses  State                 Source
  /Users/…/Marilyn Manson/1994 Portrait of an…          21  21 attached           flaccompagnon
  /Users/…/Ozzy Osbourne/1988 No Rest for the…          12  10 attached, 2 stale  flaccompagnon
  /Volumes/OldDrive/…                                    4  4 waiting             flaccompagnon
  in all: 305 attached, 2 stale, 4 waiting
```

Les trois états sont distincts : **attached**, rattaché et actuel ; **stale**, obsolète pour le fichier actuel ; **waiting**, en attente d’un rattachement. Une donnée obsolète ne doit pas servir de verdict actuel.

<div id="what-an-album-page-says-about-it" data-legacy-anchor></div>

## Le résumé sur une page d’album

La page d’album présente un résumé lorsqu’il existe des résultats pertinents, par exemple :

```text
Antichrist Superstar

  Marilyn Manson
  1996
  /Users/…/Marilyn Manson/1996 Antichrist Superstar [FLAC] [16B-44kHz]
  checked: 16 intact · flaccompagnon: 16 MD5 matches
```

Les méthodes sont nommées parce qu’elles vérifient des propriétés différentes. Un dénominateur tel que `9 of 12 intact` apparaît quand la méthode ne couvre pas tout l’album. Ne pas lire le nombre de fichiers intacts comme une promesse d’analyse acoustique ou de comparaison avec un original externe.

<div id="where-it-is-all-stored" data-legacy-anchor></div>

## Où les données sont conservées

Les analyses attribuées vivent dans `conclusions.json`, séparément du catalogue de tags reconstructible. Le rapport d’origine n’est plus nécessaire après ingestion réussie ; sa suppression ne retire pas les mesures conservées. `import --forget` retire explicitement les analyses sélectionnées. `reset` enlève `catalog.json` et les racines suivies qu’il contient, mais conserve conclusions, données personnelles et sources.

`--data` sélectionne le dossier de données pour une commande ; `AEDE_HOME` le fait par l’environnement. Sauvegarder les magasins persistants et la musique originale séparément. Voir [sécurité des données](manual/catalog.md), [import](cli/import.md) et [backup](cli/backup.md).

<div id="reusing-acoustic-reports" data-legacy-anchor></div>

## Réutiliser les rapports acoustiques

Les rapports valides sont réutilisés selon l’identité et les dates décrites plus haut. La référence [analyze](cli/analyze.md) détaille `--force`, `--show-results`, disposition et remplacement des rapports. La référence [import](cli/import.md) détaille fichiers/dossiers, états en attente ou obsolètes et suppressions explicites.
