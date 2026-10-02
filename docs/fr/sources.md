<div id="what-other-sources-say" data-legacy-anchor></div>

# Ce que disent les autres sources

Trois voix coexistent dans ce programme. Les distinguer est au cœur du projet :

- **ce que disent les fichiers** — le catalogue, reconstruit depuis vos dossiers à chaque scan ;
- **ce que vous dites** — favoris, notes, évaluations et tags, conservés dans un fichier qu’un nouveau scan ne modifie jamais ;
- **ce que dit un tiers** — MusicBrainz, Wikipédia, Fanart.tv et les autres sources consultées explicitement, dans `sources.json`.

Une valeur provenant d’une source **reste à côté de vos tags et ne les remplace jamais**. Aucune réécriture ni fusion n’est effectuée. Chaque information conserve son auteur et sa date, et peut être supprimée. Si une source contredit vos tags, les deux valeurs sont affichées et `doctor` signale le désaccord : déterminer laquelle a raison dépasse le rôle de ce programme.

<div id="querying-musicbrainz" data-legacy-anchor></div>

## Interroger MusicBrainz

```sh
aede fetch                      # all artists and all albums
aede fetch manson               # only matching the name (artist or release)
aede fetch ~/Music/Alastis      # only what is on this shelf
aede fetch --dry-run            # show what would be requested without sending anything
aede fetch --full               # re-query what is already kept
```

Un nom et un dossier répondent à deux questions différentes ; `fetch` accepte les deux. Voir [Restreindre une exécution à un dossier](#restricting-an-execution-by-folder).

**Artistes et albums sont traités dans une seule passe**. Les albums constituent la partie la plus importante : aucun tag n’indique l’origine d’un musicien, donc les informations MusicBrainz sur un artiste ne peuvent qu’être ajoutées à côté de votre bibliothèque. Pour un album, la situation diffère : Picard enregistre `RELEASETYPE`, `DATE` et `LABEL`. Vos fichiers présentent ainsi une information, MusicBrainz une autre, et elles peuvent diverger. C’est ce désaccord, particulièrement pertinent pour les albums, qui justifie ce modèle de stockage.

La date est le cas le plus courant. Votre tag `DATE` indique 1997, l’année de la réédition que vous avez extraite ; MusicBrainz indique que l’album est paru pour la première fois en 1959. Aucune des deux valeurs n’est fausse. Aède affiche les deux plutôt que d’en choisir une.

**Une requête par album, quels que soient vos tags**. S’ils contiennent l’identifiant MusicBrainz de l’album, la réponse concerne précisément cet identifiant et inclut le label dans la même requête. S’ils contiennent seulement l’identifiant du groupe de sorties (`release-group`), la réponse reste précise mais ne contient pas de label : un groupe de sorties n’en possède pas. Lui attribuer celui du premier pressage correspondant reviendrait à appliquer le label d’une édition à l’album dans son ensemble. Sans aucun de ces identifiants, le titre et l’artiste de l’album sont recherchés ; la réponse est évaluée puis refusée si son score est inférieur à 70 %, plutôt que devinée.

MusicBrainz autorise **une requête par seconde**. Une grande bibliothèque prend donc du temps à parcourir. Le programme annonce la durée estimée avant de commencer et demande une confirmation au-delà de 20 artistes. Il enregistre chaque réponse : interrompre une exécution ne fait perdre aucune réponse déjà obtenue et la suivante ne demande que les informations restantes.

Le serveur de recherche peut parfois répondre avec un code temporaire `503`, même si la limite de fréquence est respectée. Le programme attend et tente à nouveau, jusqu’à trois essais avec des délais croissants. Il ne s’arrête que si les refus persistent.

Une deuxième exécution ne redemande pas ce qui est déjà conservé. `--full` **autorise une nouvelle interrogation** ; cette option est également nécessaire après une mise à jour qui lit un champ auparavant non enregistré :

```sh
aede fetch --full manson
```

Certains artistes ne produisent aucune donnée enregistrée : c’est un fonctionnement normal. Une réponse qui ne concerne pas clairement votre artiste est ignorée plutôt qu’interprétée au hasard.

<div id="prose-language" data-legacy-anchor></div>

## Langue des textes

Le résumé est récupéré dans **une seule** langue, puis conservé dans cette langue. Le choix appartient à `fetch`, pas à l’affichage ultérieur :

```sh
aede fetch --summaries                       # terminal language, English fallback
aede fetch --summaries --lang=fr             # French, English fallback
aede fetch --summaries --full --lang=fr ozzy # replaces already stored data
aede fetch --labels --summaries roadracer    # identify a label, then fetch its article
```

Sans `--lang`, le programme lit la variable `LANG` ou `LC_ALL` du terminal (`fr_FR.UTF-8` pour le français). **L’anglais reste toujours le dernier recours**, sans être exclu : pour de nombreux artistes, c’est la seule langue dans laquelle un article existe. La passe annonce les langues recherchées avant d’envoyer la requête :

```
searching for articles in fr, en, in that order
```

Un artiste sans article français reçoit la version anglaise. L’attribution sous le paragraphe précise quelle source a été consultée :

```
  https://en.wikipedia.org/wiki/Ozzy_Osbourne — in en — CC BY-SA 4.0
  aede fetch --summaries --full --lang=fr "ozzy osbourne" asks for another
```

La dernière ligne permet de distinguer une préférence ignorée, l’absence d’article dans votre langue et des données obtenues avant que vous définissiez cette préférence.

`--full` est nécessaire pour remplacer un texte déjà enregistré.

<div id="identified-or-matched" data-legacy-anchor></div>

## Identification exacte ou correspondance

Si vos fichiers ont été traités avec **Picard**, ils contiennent déjà des identifiants MusicBrainz. `fetch` les utilise et interroge directement l’artiste, sans chercher son nom.

La correspondance est **exacte** : la réponse concerne l’identifiant demandé. Le registre affiche donc `identified` plutôt qu’un pourcentage. Un pourcentage mesure la qualité du résultat de recherche, jamais le fait qu’un artiste serait « correct à 88 % ».

La **réponse est aussi plus complète** : un résultat de recherche est abrégé, alors qu’une interrogation directe renvoie l’entité entière, par exemple l’activité actuelle du groupe ou la précision utilisée par MusicBrainz pour distinguer des artistes homonymes.

Sans identifiant, le nom est recherché et le résultat est accompagné d’un score. En dessous de 70 %, rien n’est enregistré. Deux réponses aussi plausibles l’une que l’autre sont refusées plutôt que départagées arbitrairement :

```
? Nirvana: several answers are equally good: Nirvana, Nirvana (UK)
? Sh: the closest was "Shellac" at 61%, not close enough
```

Une correspondance approximative est enregistrée comme information attribuée, mais ne devient pas une identité du graphe tant que vous n’avez pas confirmé qu’il s’agit de la bonne. Une réponse MusicBrainz exacte qui contredit un identifiant déjà présent dans vos tags suit le même processus de validation :

```sh
aede review                         # unresolved claims
aede review --interactive           # compare and decide one claim at a time
aede review manson                  # narrow them by entity name
aede review --accept=<ID>           # trust this exact claim
aede review --reject=<ID>           # keep it visible, but never traverse it
aede review --undo=<ID>             # return the decision to pending
aede review --all                   # include accepted and rejected claims
```

La vue interactive présente l’identité locale et la proposition de la source avant de demander une décision. Utilisez `A` pour accepter, `R` pour refuser, `S` pour laisser la proposition en attente, `P` pour revenir en arrière, `U` pour annuler une décision existante et `Q` pour arrêter. Chaque décision est enregistrée immédiatement : une session interrompue peut simplement être reprise.

Accepter ne transforme jamais `matched 92%` en `identified` : la confiance d’origine reste visible. L’acceptation enregistre seulement votre accord sur cette combinaison précise d’entité, de source et d’identifiant, pour la navigation et les requêtes. Un autre candidat proposé par une récupération ultérieure nécessite sa propre décision. Refuser ne supprime pas l’information ; tous les choix peuvent être annulés.

<div id="manual-corrections" data-legacy-anchor></div>

## Corrections manuelles

Les valeurs récupérées ne sont pas définitives. Les enregistrements sont indexés par **(entité, source)**. Une information placée sous votre propre source ne sera donc pas remplacée par MusicBrainz : un futur `aede fetch --full` ajoutera sa ligne à côté de la vôtre, sans la toucher.

Générez un document avec les bonnes clés :

```sh
aede sources --template --source=manual --output=fix.json "Kind of Blue"
```

Renseignez les champs souhaités (`null` si une donnée manque) :

```json
{
  "entity": "release:miles davis|kind of blue|/Users/you/Music/Miles/Kind of Blue",
  "source": "manual",
  "facts": {
    "primary_type": "Album",
    "first_released": "1959-08-17",
    "label": "Columbia",
    "secondary_types": []
  }
}
```

Importez-le :

```sh
aede sources --import=fix.json
```

`--source=manual` protège cette entrée. Tout nom convient (`manual`, votre nom, `discogs`…), à condition d’éviter `musicbrainz` : une source ne remplace que ce qu’**elle-même** avait déclaré.

Pour appliquer la correction et les décisions de validation à un autre catalogue Aède, exportez les règles plutôt que toute la couche des informations récupérées :

```sh
aede rules --export --output=rules.json
aede rules --import=rules.json
```

Ce document versionné inclut les enregistrements de sources manuelles, les identités de sources acceptées ou refusées et les exclusions précises de crédits, ainsi que les règles personnelles de classement et de relations. L’import fusionne ces choix ; il ne copie ni biographies, ni images, ni historique d’écoute, et ne modifie aucun fichier audio. Utilisez plutôt `aede export --graph` si vous souhaitez conserver l’ensemble du catalogue, des sources et des données personnelles pour les analyser.

<div id="viewing-and-deleting" data-legacy-anchor></div>

## Consulter et supprimer

```sh
aede sources                    # one line per source: how much, how much lands
aede sources --list             # every record, and whether the catalog places it
aede sources --export --output=backup.json
aede sources --forget --source=musicbrainz
aede review                     # ambiguous identities needing a decision
```

`aede artist` et `aede album` affichent un bloc « What sources say » (ce que disent les sources), qui compare chaque valeur à vos tags :

```
  Source                        Field           Says        Your tags
  ────────────────────────────  ──────────────  ──────────  ────────────────────
  musicbrainz 88% · 2 days ago  release type    Album       nothing in your tags
  musicbrainz 88% · 2 days ago  first released  1959-08-17  matches your tags
  musicbrainz 88% · 2 days ago  label           Blue Note   Columbia
  musicbrainz: https://musicbrainz.org/release-group/c9fdb94c-…
```

**La dernière ligne indique l’adresse de la source ayant fourni la réponse**, avec une adresse par source. Cet identifiant est indispensable pour une vérification manuelle : ouvrir la page, exécuter la requête avec `curl` ou corriger les données à la source.

L’identifiant figure dans l’URL et peut être copié séparément. L’adresse d’un album pointe vers son **groupe de sorties**, qui représente l’album dans son ensemble plutôt qu’un pressage précis.

Trois situations sont distinguées : une valeur confirmée par vos tags, une valeur contradictoire et une valeur absente de vos tags.

Deux libellés méritent une précision :

- `from` correspond à la zone géographique indiquée par MusicBrainz : pays, ville ou région.
- `note` correspond à la précision servant à distinguer des artistes homonymes.

<div id="secondary-passes-and-combined-runs" data-legacy-anchor></div>

## Passes complémentaires et exécutions combinées

Exécuter `aede fetch` seul interroge MusicBrainz pour vos artistes et albums. Les options activent des passes complémentaires sur le catalogue identifié :

```sh
aede fetch --summaries      # the Wikipedia article behind each wikidata link
aede fetch --discography    # everything MusicBrainz credits to each artist
aede fetch --lyrics         # missing words from LRCLIB, as .lrc sidecars
aede fetch --covers         # the front image of every album that has none
aede fetch --portraits      # Wikidata first, then Fanart.tv as fallback
aede fetch --labels         # identify record labels through MusicBrainz
aede fetch --credits        # recording/work credits and exact-edition credits, where IDs exist
aede fetch --logos          # artist and identified-label logos from Fanart.tv
aede fetch --fanart         # every supported Fanart.tv image family
```

`--credits` ne recherche jamais un titre : cette passe suit seulement un `MUSICBRAINZ_RECORDINGID` attaché à un enregistrement local, ou un identifiant de sortie présent sur une édition locale. Une interrogation d’enregistrement conserve ses rôles directs, les œuvres liées et leurs rôles créatifs. Une interrogation distincte de l’édition conserve les crédits d’artiste au niveau de la sortie, sans les attribuer à tous les enregistrements. La passe préserve notamment les noms tels que crédités, les instruments et qualificatifs, les dates, l’ordre et les identifiants de relations. Les anciennes données obtenues avec `--recordings` sont actualisées une fois dans cette forme plus riche ; `--recordings` reste un alias accepté.

Si une œuvre liée contient une relation MusicBrainz explicite indiquant qu’elle fait partie d’une autre œuvre, l’identifiant et le titre du parent, le qualificatif de mouvement ou d’acte et l’ordre sont conservés, avec la même source et les mêmes limites de confiance. `aede work <parent ID>` et `query work:<parent ID>` permettent alors d’atteindre les mouvements présents localement. Les tags locaux d’œuvre, de regroupement et de mouvement restent distincts et sont affichés séparément ; ils n’établissent pas seuls l’identité d’un parent. Une interrogation d’enregistrement déjà terminée n’est pas automatiquement répétée pour cet ajout : utilisez `aede fetch --credits --full <album folder>` pour actualiser une édition choisie.

Pour voir quels enregistrements attendent encore la récupération des crédits d’enregistrement et d’œuvre, lancez `aede credits`, puis `aede credits "<album>"`. Ce rapport en lecture seule distingue un enregistrement sans réponse (`waiting`), une réponse terminée sans crédit (`empty`) et des crédits attachés à une source non approuvée (`untrusted`). Le détail de l’album suggère la commande de récupération limitée au dossier pour les enregistrements en attente. Ceux qui n’ont pas d’identifiant MusicBrainz d’enregistrement local doivent d’abord être identifiés. Ce rapport ne compte pas encore les interrogations des crédits d’édition. `aede fetch --credits --dry-run` présente les requêtes en attente pour les deux périmètres.

Une fois les crédits approuvés disponibles, un contributeur absent des tags locaux peut être ouvert avec `aede artist <MusicBrainz artist ID>`. La fiche indique explicitement son origine externe : elle navigue vers les enregistrements et albums présents localement et vers les œuvres portant des crédits créatifs, sans créer artificiellement un artiste dans le catalogue. `aede search <name>` inclut ces contributeurs dans une section distincte ; les homonymes restent différenciés par leur identifiant. Une proposition de source en attente ou refusée ne crée pas cette identité navigable.

**Chaque passe accepte des noms et des dossiers** :

```sh
aede fetch --discography "pink floyd"
aede fetch --covers manson portishead      # a list is fine
aede fetch --summaries mika                # nothing here matches mika
aede fetch --lyrics ~/Music/Alastis        # that shelf, whatever it is called
```

Un nom cible un artiste par son nom, et un album par son titre **ou** son artiste.

<div id="restricting-an-execution-by-folder" data-legacy-anchor></div>

## Restreindre une exécution à un dossier

Toutes les options de `fetch` acceptent des dossiers, selon la même logique que `check`, `playlist` et `fingerprint` :

```sh
aede fetch ~/Music/Alastis                 # the artists and albums on that shelf
aede fetch --lyrics ~/Desktop/test/Alastis # the words, for those tracks only
aede fetch --covers --lyrics ~/Music/80s   # several passes, one shelf
aede fetch ozzy ~/Music/80s                # that person, on that shelf
```

**Tout chemin existant sur le disque est interprété comme un dossier ; tout autre argument est interprété comme un nom**. L’exécution affiche les dossiers retenus avant de lancer les requêtes :

```
  only what is under /Users/you/Music/Alastis

Lyrics
```

Les deux critères s’appliquent indépendamment : le nom définit **qui**, le dossier définit **où**.

Un dossier que le catalogue n’a pas scanné est **refusé** :

```
no file in the catalog is under "~/Music/New".
It is on disk, so this catalog was scanned 3 days ago and has not seen it — a folder added since is not in it yet.
Add it: aede scan "~/Music/New"
```

Les options peuvent être combinées ; les passes s’exécutent successivement :

```sh
aede fetch --covers --discography          # both, in one go
aede fetch --summaries --discography --covers --dry-run
```

L’ordre part de l’artiste puis s’en éloigne : identité, discographie, visuels.

<div id="querying-wikipedia" data-legacy-anchor></div>

## Interroger Wikipédia

MusicBrainz ne stocke pas de biographies complètes, mais conserve un lien vers elles. Leur récupération est une **option de** `fetch` :

```sh
aede fetch --summaries          # follow it, for every artist already fetched
aede fetch --summaries --full   # ask again about what is already held
```

```
→ 402 stored, 3 left alone, 0 failed
  381 of them have a wikidata link — aede fetch --summaries reads the article
```

Il s’agit d’une **deuxième passe sur les données de** `fetch`, pas d’une nouvelle recherche de nom sur Wikipédia. Le programme lit le lien `wikidata` d’un artiste ou d’un label, interroge Wikidata pour trouver l’article correspondant, puis récupère le premier paragraphe sur Wikipédia, soit deux requêtes par entité. `fetch --labels` demande désormais à MusicBrainz le lien Wikidata de chaque label. Un label récupéré avec une ancienne version d’Aède peut être actualisé avec `aede fetch --labels --full <name>` avant de lancer `aede fetch --summaries <name>`. Si MusicBrainz ne fournit pas de lien Wikidata, ou si Wikidata ne propose aucun article dans les langues demandées, cette passe ne peut afficher aucune biographie Wikipédia.

Pour un label sans biographie Wikipédia, ouvrir sa page consulte le lien Discogs conservé dans son enregistrement MusicBrainz :

```sh
aede label roadracer
```

Le profil Discogs est conservé dans `sources.json` avec sa source, sa page et la date de récupération. Ouvrir la page vérifie le profil actuel et remplace la copie enregistrée si une réponse est disponible. `--offline` évite cette requête. Si la vérification échoue, la page du label affiche le profil enregistré avec un avis de mise à jour ; `--offline` affiche également cet avis si la copie date de plus de cinq heures. Les autres vues de sources et les exports masquent encore un profil aussi ancien. Aède vérifie l’identifiant et le nom du label Discogs et ne recherche jamais un label par son nom sur Discogs. Les références de labels comme `[l30552]` et d’artistes comme `[a1258936]` sont remplacées par leurs noms, indépendamment du catalogue local ; les balises de présentation telles que `[b]` sont supprimées. Un profil déjà enregistré contenant des codes d’artistes non résolus est retraité lors de la prochaine visite réussie. Le texte d’origine est normalement en anglais : Discogs ne fournit pas de version française de ce champ, même sur son site français. L’affichage inclut « Data provided by Discogs. » et la page source. Les [conditions de l’API Discogs](../design/discogs.md) limitent l’ancienneté des informations affichées ; le repli hors ligne est donc soumis à une limite de conformité si aucune actualisation n’est possible pendant longtemps.

La page du label préserve les retours à la ligne Discogs et les lignes vides entre paragraphes. Elle met en évidence la ligne du code de label si le style du terminal est activé. Les lignes longues sont renvoyées à la ligne sans fusionner les paragraphes. Cette présentation s’applique également aux profils déjà enregistrés, y compris hors ligne.

Les articles sont recherchés **d’abord dans la langue du système**, indiquée par `LANG`, puis en anglais.

<div id="credit-is-part-of-the-text" data-legacy-anchor></div>

## L’attribution fait partie du texte

Les articles Wikipédia sont sous licence **CC BY-SA**. Utiliser leur texte exige de citer la source et la licence. Aède conserve le paragraphe, la page, la langue et la licence comme **un ensemble indissociable** :

```
  Marilyn Manson is an American rock band formed in Fort Lauderdale,
  Florida, in 1989.
  https://en.wikipedia.org/wiki/Marilyn_Manson_(band) — CC BY-SA 4.0
```

`aede sources --forget --source=wikipedia` supprime l’ensemble de ces informations.

<div id="images-already-present-in-your-files" data-legacy-anchor></div>

## Images déjà présentes dans vos fichiers

Avant tout téléchargement, une solution locale existe. Un album sans fichier `cover.jpg` dans son dossier peut extraire son visuel **depuis les fichiers audio** :

```sh
aede extract                    # write it out, everywhere it is missing
aede extract ~/Music/Manson     # only under these folders
aede extract --images           # the back and the booklet too, in artwork/
aede extract --dry-run          # say which folders, write nothing
```

**Le programme lit vos fichiers sans jamais les modifier**. Aucune écriture n’a lieu à l’intérieur des fichiers audio. L’image est créée à côté, comme une playlist ou un spectrogramme. Cette extraction fonctionne pour **tous les formats** : FLAC, ID3, MP4 et Ogg.

L’extraction s’exécute **par dossier**. Un dossier contenant déjà une image est ignoré.

<div id="other-embedded-visuals" data-legacy-anchor></div>

## Autres visuels intégrés

Outre la couverture avant, `--images` extrait le dos, le livret ou le disque dans un sous-dossier `artwork/`, pour ne pas perturber la détection de la couverture avant par les lecteurs :

```
/music/Miles Davis/Kind of Blue/
    cover.jpg
    artwork/
        back.jpg
        booklet-01.jpg
        booklet-02.jpg
        media.jpg
```

<div id="album-cover-art" data-legacy-anchor></div>

## Couvertures d’albums

Récupérer les visuels depuis Cover Art Archive, lié à MusicBrainz :

```sh
aede fetch --covers                 # albums with no cover, at 1200 px
aede fetch --covers --size=500      # 250, 500, 1200 or original
aede fetch --covers --images        # the back and the booklet too, in artwork/
aede fetch --covers --dry-run       # say which albums, ask nothing
```

**Exécutez d’abord** `aede extract`. `--covers` télécharge uniquement si aucune image n’est présente, ni intégrée aux fichiers ni dans le dossier. Il n’existe pas d’option `--replace`, afin d’éviter les écrasements accidentels.

Les images téléchargées sont enregistrées dans `cover.jpg`, à côté des pistes audio. Les fichiers audio ne sont jamais modifiés. Seuls des fichiers d’image valides, JPEG ou PNG, sont enregistrés : une page HTML signalant une erreur ne devient pas une fausse image corrompue.

<div id="fanarttv-artwork" data-legacy-anchor></div>

## Visuels Fanart.tv

Fanart.tv complète Cover Art Archive avec des visuels d’artistes, des logos de labels et d’autres images d’albums. Il nécessite une clé d’application gratuite dans `AEDE_FANARTTV_KEY` :

```sh
export AEDE_FANARTTV_KEY="your-key"
aede fetch --fanart
```

`--fanart` active toutes les familles prises en charge dans une seule passe :

| Famille | Règle de sélection | Destination |
| --------------- | ----------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| Logo d’artiste | HD en priorité, résolution standard en repli | `logo.jpg` ou `logo.png` à côté de la musique de l’artiste, ou dans `assets/artists/<MusicBrainz ID>/` |
| Logo de label | Logo disponible le plus apprécié | `assets/labels/<MusicBrainz ID>/` |
| Portrait d’artiste | Portrait le plus apprécié | `artist.jpg` ou `artist.png` à côté de la musique de l’artiste, ou dans `assets/` |
| Fond | **4K en priorité**, 1080p seulement si aucun fond 4K n’existe | `background.jpg` ou `background.png` à côté de la musique de l’artiste, ou dans `assets/` |
| Bannière | Bannière large la plus appréciée | `banner.jpg` ou `banner.png` à côté de la musique de l’artiste, ou dans `assets/` |
| Couverture d’album | Couverture d’album Fanart.tv la plus appréciée | `artwork/cover.jpg` ou `artwork/cover.png` dans le dossier de l’album |
| cdART | Une image par disque | `artwork/media.jpg`, ou fichiers numérotés `media-01.jpg`, `media-02.jpg`, etc. |

Partez de toutes les familles puis excluez uniquement celles que vous ne souhaitez pas :

```sh
aede fetch --fanart --no-logo
aede fetch --fanart --no-label-logo --no-banner
aede fetch --fanart --no-portrait --no-background
aede fetch --fanart --no-album-cover --no-cdart
```

La liste complète des exclusions est `--no-logo`, `--no-label-logo`, `--no-portrait`, `--no-background`, `--no-banner`, `--no-album-cover` et `--no-cdart`. Une exclusion sans `--fanart` est refusée plutôt qu’ignorée. Les demandes contradictoires telles que `--fanart --logos --no-logo` ou `--fanart --banners --no-banner` sont également refusées.

Les formes plus ciblées restent disponibles pour la compatibilité et les exécutions limitées :

```sh
aede fetch --logos              # artist and identified-label logos only
aede fetch --logos --banners    # the same artist response also yields a banner
```

Chaque famille possède son propre enregistrement de fin de traitement. Si vous excluez les fonds aujourd’hui, un futur `aede fetch --fanart --no-portrait` pourra les récupérer sans redemander les familles d’images déjà terminées. `--full` demande volontairement une nouvelle récupération. Les fichiers JPEG et PNG existants ne sont jamais écrasés. Les octets téléchargés sont vérifiés comme images avant toute écriture.

<div id="missing-from-the-shelf" data-legacy-anchor></div>

## Ce qui manque dans la bibliothèque

```sh
aede fetch --discography    # browse everything credited to each artist
aede missing                # what is credited to them and not here
aede missing davis          # narrowed, by artist or by title
```

`missing` n’effectue **aucune requête réseau**. Il calcule son résultat à partir des informations de discographie déjà enregistrées.

```
Missing

  Artist       Album              Year
  ───────────  ─────────────────  ────
  Miles Davis  Sketches of Spain  1960
  Miles Davis  Bitches Brew       1970
  2 studio albums
  9 records left out — --all lists them here, with the reason for each: singles,
  live records, compilations, demos, and anything you set aside
```

<div id="exclude-or-re-include-an-album" data-legacy-anchor></div>

## Exclure ou réintégrer un album

```sh
aede missing --forget "Sweet Dreams"            # stop listing it
aede missing --list                             # what you set aside
aede missing --list manson                      # narrowed, by artist or by title
aede missing --forget "Sweet Dreams" --remove   # put it back
```

Les choix sont enregistrés dans `user.json` et indexés par l’identifiant MusicBrainz du groupe de sorties.

Le filtre par défaut ne conserve que les **albums studio** d’artistes dont la bibliothèque locale possède au moins un album complet.

<div id="identifying-a-file-by-acoustic-fingerprint" data-legacy-anchor></div>

## Identifier un fichier par empreinte acoustique

Une empreinte acoustique est calculée depuis le signal audio décodé pour identifier des fichiers mal renseignés ou dont les métadonnées manquent.

```sh
aede fingerprint                # decode and work out what the audio is
aede fingerprint ~/Music/Rips   # only under these folders
aede fingerprint --full         # every file, not just the nameless ones
aede fingerprint --list         # print what is stored, to compare it
aede fetch --identify           # ask AcoustID what it hears
```

Par défaut, les fichiers correctement renseignés ou possédant déjà un identifiant MusicBrainz sont ignorés.

<div id="comparison-and-duplicate-detection" data-legacy-anchor></div>

## Comparaison et détection des doublons

`aede doctor` regroupe les fichiers partageant la même empreinte acoustique pour identifier des enregistrements strictement identiques, quels que soient leurs tags :

```
  ! the same audio — 2 files are the same recording (379.1 kB recoverable)
      /music/Copie/track07.flac
      /music/Original/01.flac
```

Prérequis : Chromaprint, par `ffmpeg` ou `fpcalc`, et une clé d’API AcoustID définie dans `AEDE_ACOUSTID_KEY`. Le système identifie et suggère des correspondances sans jamais modifier les fichiers audio.

<div id="what-musicbrainz-does-not-contain" data-legacy-anchor></div>

## Ce que MusicBrainz ne contient pas

- **Pas de biographie complète** : seules sont fournies le type d’artiste, la zone, les dates et une précision d’homonymie. Wikipédia et Wikidata complètent ces informations.
- **Aucun écrasement de vos fichiers** : les données récupérées restent indépendantes des tags d’origine. Supprimer `sources.json` ne modifie pas vos fichiers locaux.
