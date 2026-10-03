<div id="queries-querying-your-cdthèque-with-precision" data-legacy-anchor></div>

# Requêtes : interroger sa CDthèque avec précision

Les options ordinaires d’une commande se combinent par `AND` : tous les critères doivent être remplis. Une véritable grammaire de requête permet d’aller plus loin. Des options seules n’expriment pas `--genre metal OR --genre jazz`, « tout sauf ce label » ou « entre 1990 et 1999 ». Le moteur de requête unifié d’Aède permet ces trois recherches.

Retrouver des albums des années 1990 peu écoutés, une note personnelle sur une édition japonaise ou des pistes à emporter utilise la même syntaxe.

```sh
aede query "genre:metal year:1990..1999 label:earache"
aede query "(artist:ozzy OR artist:dio) album.rating:>=4"
aede query "loved played:0" --m3u          # favoris jamais écoutés
aede query "lossless:false size:>50000000" # gros fichiers avec perte
```

<div id="available-fields-and-graph-relations" data-legacy-anchor></div>

## Champs disponibles et relations du graphe

La recherche parcourt la structure relationnelle du catalogue. Un préfixe précise le niveau visé (`album.`, `artist.`, `track.`) ; sans préfixe, le contexte de la commande détermine le niveau.

| Champ | Type | Description | Exemples |
| :--- | :--- | :--- | :--- |
| `title` | Texte | Titre de piste | `title:Interstellar` |
| `artist` / `albumartist` | Texte | Interprète ou artiste de l’album | `artist:Coltrane`, `albumartist:Metallica` |
| `album` | Texte | Titre d’album | `album:"Kind of Blue"` |
| `recording` | Texte/ID | Identité de l’interprétation enregistrée | `recording:9f…`, `recording:"So What"` |
| `work` | Texte/ID | Œuvre interprétée | `work:"All Along the Watchtower"` |
| `releasegroup` | Texte/ID | Identité de l’album commune à ses éditions | `releasegroup:5c…` |
| `genre` | Texte | Genre musical | `genre:=Jazz`, `genre:metal` |
| `label` | Texte | Label discographique | `label:"Blue Note"` |
| `year` | Intervalle/nombre | Année de parution | `year:1994`, `year:1985..1995` |
| `duration` | Durée | Durée de la piste | `duration:..4:00`, `duration:3:30..5:00` |
| `size` | Octets | Taille du fichier en octets | `size:>50000000` |
| `codec` / `format` | Texte | Codec audio ou conteneur | `codec:flac`, `format:mp3` |
| `bitrate` / `samplerate` | Nombre | Débit / fréquence d’échantillonnage | `bitrate:>=320`, `samplerate:96000` |
| `lossless` | Booléen | Compression sans perte | `lossless:true`, `-lossless` |
| `compilation` | Booléen | Compilation de plusieurs artistes | `compilation:true` |
| `played` | Compteur | Nombre d’écoutes | `played:0`, `played:>=10` |
| `comment` | Texte | Commentaire dans les tags du fichier | `comment:"vinyl rip"` |
| `lyrics` | Texte | Paroles intégrées ou dans un fichier annexe | `lyrics:train` |
| `path` | Texte | Chemin absolu du fichier | `path:"/FLAC/Ozzy"` |
| `instrument` | Texte | Instrument ou attribut d’un crédit | `instrument:"electric guitar"` |
| **Annotations** | | | |
| `rating` | Nombre | Note personnelle en étoiles (1–5) | `rating:>=4`, `album.rating:5` |
| `loved` | Booléen | Favori personnel | `loved`, `-loved`, `track.loved` |
| `tag` | Texte | Étiquette personnelle | `tag:vinyl`, `album.tag:audiophile` |
| `note` | Texte | Texte de la note Markdown personnelle | `note:remaster`, `artist.note:concert` |

<div id="credits-and-who-did-what" data-legacy-anchor></div>

### Crédits : qui a fait quoi ?

Le catalogue relie des entités plutôt que de les réduire à une seule table. Aède distingue les rôles créatifs pour rechercher dans les crédits :

- **Champs de rôle :** `composer`, `lyricist`, `producer`, `engineer`, `performer`, `conductor`, `orchestra`, `choir`, `ensemble`, `soloist`, `remixer`, `featured`, `mainartist`.
- **Classe d’interprétation (`performing`) :** participants à l’interprétation, chef d’orchestre compris ; un compositeur crédité uniquement pour l’écriture n’entre pas dans cette classe.
- `orchestra` correspond aussi au rôle MusicBrainz `performing orchestra` ; `soloist` correspond à un tag local de soliste ou à une relation de source d’interprète/instrument/voix explicitement marquée `solo`.
- **Crédit général (`artist:`) :** n’importe quel crédit de piste ou d’album, quel que soit son rôle. `artist:ozzy artist:"zakk wylde"` exige les deux personnes parmi les crédits ; les champs de rôle précisent leur contribution.

```sh
aede query "composer:rhoads mainartist:ozzy"   # Ozzy interprétant ce que Randy a écrit
aede query "producer:\"rick rubin\" year:1990.."
aede query "guest:\"zakk wylde\""               # invité sur un album hors compilation
aede query "compilationartist:\"miles davis\""  # interprète sur une compilation
aede query "contributor:\"rick rubin\""         # contribution hors interprétation
aede query "with:\"zakk wylde\""                # co-interprète sur la même piste
aede query 'work:MUSICBRAINZ_PARENT_WORK_ID soloist:"A Soloist"' # mouvements identifiés
aede query 'work:"Symphony No. 5" movement:Allegro'       # tags WORK/mouvement locaux
```

Les champs d’identité (`recording`, `work`, `releasegroup`) acceptent le titre affiché ou l’identifiant MusicBrainz. `instrument` recherche les attributs d’un crédit. `guest`, `compilationartist`, `contributor` et `with` suivent les liens de participation pour retrouver les pistes correspondantes. Les alias `collaborator` et `compilation-artist` sont aussi acceptés.

Ces champs consultent les tags locaux explicites et les relations obtenues par `aede fetch --credits`. Une donnée de source rattachée par identifiant exact et sans conflit est utilisable automatiquement. Une proposition approximative ou contradictoire ne l’est qu’après `aede review --accept=<ID>`. Les propositions en attente ou rejetées restent des preuves visibles ; elles ne deviennent pas discrètement des relations de recherche. L’index des sources est construit une fois par requête, sans reparcourir toute la bibliothèque pour chaque piste et chaque terme.

<div id="strict-semantics-and-boolean-symmetry" data-legacy-anchor></div>

### Règles strictes et équivalence des booléens

1. **Erreur explicite plutôt que résultat vide trompeur :** nommer une entité absente du catalogue, par exemple un genre inexistant ou un artiste inconnu, produit une **erreur**. Une liste vide signifie « aucune piste ne remplit ces critères » ; l’erreur signifie « ce terme n’existe pas dans le catalogue ».
2. **Équivalence des booléens :** `lossless:false` et `-lossless` expriment la même recherche.

<div id="lyrics-sidecars-and-text-precision" data-legacy-anchor></div>

## Paroles, fichiers annexes et recherche textuelle

Le champ `lyrics:` consulte les paroles intégrées au conteneur audio, par exemple Vorbis `LYRICS`, ID3 `USLT` ou atome `©lyr`, ainsi que les fichiers `.lrc` placés près de la piste.

```sh
aede query "lyrics:train"
```

Le scan indexe les tags intégrés : la requête n’a pas à rouvrir l’audio pour les lire. Les `.lrc` annexes sont ouverts et analysés au moment de la recherche. Pour afficher les paroles complètes et leurs repères temporels :

```sh
aede track "Crazy Train" --lyrics
```

<div id="searching-what-you-wrote-annotations--scopes" data-legacy-anchor></div>

## Retrouver ses annotations et choisir leur portée

Les annotations ajoutées par `aede note`, `aede rate` ou `aede tag` sont immédiatement recherchables.

```sh
aede query "tag:vinyl"              # pistes portant cette étiquette
aede query "album.tag:vinyl"        # pistes dont l’album porte cette étiquette
aede query "note:remaster"          # notes contenant « remaster »
aede query "artist.note:live"       # notes de l’artiste
aede query "album.rating:>=4 played:0"
```

<div id="scope-isolation-and-diagnostic-guidance" data-legacy-anchor></div>

### Portées distinctes et aide au diagnostic

La portée (`track`, `album`, `artist`) fait partie de la question. `rating`, `tag` ou `note` sans préfixe interroge la **piste**. Si vous avez noté l’album et aucune de ses pistes, `aede query "rating"` ne correspond pas à cette note d’album.

Aède détecte les annotations présentes à un niveau voisin et propose une indication pour préciser la recherche :

```text
$ aede query "rating"
nothing matches "rating"
  1 track if you ask it of the album — that is where you wrote it
  aede query "album.rating"
```

<div id="the-loved-inheritance-exception" data-legacy-anchor></div>

### L’exception d’héritage de `loved`

Un favori (`loved`) exprime un intérêt général pour une piste, un album ou un artiste. Il n’est pas nécessaire de marquer séparément chaque piste d’un album favori.

Sans préfixe, `loved` correspond si la **piste** est favorite, si son **album** l’est ou si son **artiste** l’est :

```sh
aede query "loved played:0"   # favoris jamais écoutés, directement ou via album/artiste
```

Pour viser exclusivement un niveau, écrire `track.loved`, `album.loved` ou `artist.loved`.

<div id="querying-field-existence" data-legacy-anchor></div>

### Rechercher l’existence d’un champ

Pour rechercher la présence ou l’absence d’une annotation, sans chercher un texte particulier dedans, employer son nom seul ou précédé de `-` :

```sh
aede query "note"        # éléments portant une note personnelle
aede query "-rating"     # éléments jamais notés
aede query "tag"         # éléments portant au moins une étiquette personnelle
```

Pour chercher littéralement un mot comme « note » ou « rating », préciser un champ textuel : `title:note`.

<div id="disambiguating-comments-notes-and-lyrics" data-legacy-anchor></div>

## Distinguer commentaires, notes et paroles

Aède distingue trois origines de texte :

1. **Commentaires du fichier (`comment`) :** métadonnées placées dans le conteneur audio par un outil de tags, par exemple origine de l’extraction ou détails de l’édition.
2. **Notes personnelles (`note`) :** texte Markdown que vous écrivez, conservé séparément dans `user.json`.
3. **Paroles (`lyrics`) :** texte interprété, dans les tags ou un `.lrc`.

Avec `aede search`, ces recherches textuelles sont activées explicitement. Leur texte libre ne masque ainsi pas les résultats ordinaires sur titres et artistes :

```sh
aede search --comments "vinyl rip"
aede search --notes "remaster"
aede search --lyrics "all aboard"
```

<div id="output-formatting-and-provenance-tracking" data-legacy-anchor></div>

### Format de sortie et provenance des résultats

- **Ligne de paroles correspondante :** le résultat affiche la ligne trouvée, plutôt que tout le texte multiligne dans le tableau.
- **Provenance JSON :** la clé `found_in` indique l’origine de la correspondance : `found_in: comment`, `found_in: note` ou `found_in: lyrics`.

<div id="saved-collections--smart-playlists" data-legacy-anchor></div>

## Collections enregistrées et playlists intelligentes

Une requête réutilisée peut devenir une **collection enregistrée** :

```sh
aede collection wishlist --query "loved played:0"
aede collection wishlist                 # afficher son contenu actuel
aede collection wishlist --m3u           # exporter une playlist M3U
aede collections                         # lister les collections et nombres de pistes
aede collection wishlist --remove
```

<div id="smart-collections-vs-static-playlists" data-legacy-anchor></div>

### Collections dynamiques et playlists statiques

Une collection conserve la **formule**, pas une liste figée de fichiers. À chaque consultation ou export (`--m3u`, `--csv`, `--json`), Aède réévalue cette formule avec le catalogue actuel. Les albums nouvellement scannés et les tags mis à jour changent donc les résultats sans réécrire la définition de la collection.

La syntaxe est validée lors de l’enregistrement : une expression invalide dans `aede collection --query` est refusée immédiatement, avant de pouvoir provoquer un échec dans un export ultérieur.

## Validation et sélections vides

Les bornes numériques sont finies et positives ou nulles. `bitrate` se mesure en kbps (`bitrate:>=320`) et `samplerate` en Hz. Une durée accepte des secondes ou `minutes:secondes`, avec des minutes entières et des secondes inférieures à 60. Une plage exige au moins une borne et un ordre croissant. Les opérateurs incomplets/répétés et plus de 128 niveaux de parenthèses ou de négation sont refusés.

`played:0` sélectionne les pistes jamais écoutées ; le mot seul `played` recherche du texte. Une sélection vide réussit avec `[]` en JSON ou le seul en-tête CSV.
