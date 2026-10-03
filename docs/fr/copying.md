
<div id="copying-taking-your-cdthèque-on-the-road" data-legacy-anchor></div>

# Copier la musique pour l’emporter

`aede copy` écrit une destination distincte : baladeur, carte ou disque, sans toucher aux originaux. L’arborescence est conservée relativement aux racines : sous une racine `Music`, `Music/Artist/Album/01.flac` devient `Artist/Album/01.flac` dans la destination. Aucun reclassement automatique depuis les tags.

```sh
aede copy /Volumes/Player --dry-run
aede copy /Volumes/Player --query "loved" --verify
aede copy /Volumes/Card --collection Road --verify
```

Le dossier de base doit déjà exister. `--query` emploie la [grammaire](../querying.md), `--collection` réévalue une collection. Choisir une seule option ; sans les deux, tout le catalogue. `loved` hérite des favoris d’album/artiste ; une note d’album demande `album.rating:>=4`.

<div id="packing-the-liner-notes-and-artwork" data-legacy-anchor></div>

## Images et fichiers annexes

| `--extras` | Contenu |
| --- | --- |
| `none` | Audio seul ; une image intégrée reste dans la copie non convertie |
| `cover` (défaut) | Pochette identifiée par le catalogue |
| `images` | Images à côté de l’audio |
| `all` | Annexes : journaux, CUE, rapports, etc. |

La pochette choisie évite d’embarquer automatiquement tous les gros spectrogrammes/livrets.

<div id="navigating-fragile-filesystems" data-legacy-anchor></div>

## Noms et systèmes de fichiers

Aède teste la destination pour ses restrictions et peut adapter caractères, fins de noms et noms de périphériques réservés. `--safe-names` force l’adaptation ; `--raw-names` conserve les caractères originaux et peut échouer. Les compteurs de collision restent applicables. Options incompatibles.

Les noms adaptés sont listés. Des compteurs déterministes distinguent les collisions de fichiers et de dossiers par une comparaison conservatrice des majuscules, accents composés/décomposés courants et ponctuation. Deux albums distincts ne sont pas fusionnés après adaptation. Une copie réelle teste aussi les chemins prévus dans un dossier isolé sur la destination ; les autres équivalences ou noms refusés entraînent une erreur avant de modifier les résultats réels.

<div id="ensuring-it-arrives-intact" data-legacy-anchor></div>

## Vérifier l’arrivée

Un nouveau fichier est écrit sous un nom temporaire avant publication. Les copies simples déjà présentes à la taille attendue non nulle sont ignorées sans comparaison de contenu. Une conversion existante non vide l’est aussi. Taille/existence servent à reprendre, pas à prouver l’identité.

Chaque sortie utilise un dossier temporaire privé créé exclusivement. Les anciens noms temporaires et liens symboliques de destination ne peuvent pas détourner l’écriture. Sans `--replace`, une copie simple de taille différente ou une conversion vide est conservée et signalée. La publication atomique refuse aussi un fichier créé entre-temps par un autre écrivain ; elle exige les liens physiques du système de fichiers, vérifiés avant les transferts concernés. FAT/exFAT ne les proposent généralement pas : un `--replace` explicite permet alors la publication, mais autorise aussi le remplacement des fichiers existants. Une reprise qui ignore tous les fichiers déjà présents n’exige pas cette capacité. Ces contrôles ne protègent pas contre un processus qui remplace simultanément les dossiers parents.

`--verify` relit les copies simples **nouvellement écrites** et compare leurs octets à ceux de la source avec une mémoire bornée après vidange applicable. Les fichiers ignorés ne sont pas relus. `--replace --verify` les réécrit et vérifie volontairement.

L’espace estimé compte seulement les écritures restantes et les encodages temporaires nécessaires à `--verify-existing`. Une erreur de lecture des fichiers annexes demandés bloque le transfert ; un dossier illisible n’est pas considéré comme vide.

Les sources audio et annexes doivent être des fichiers locaux ordinaires ; une entrée périmée pointant vers un FIFO ou périphérique est refusée même en aperçu. L’encodage utilise un chemin local canonique, sans interpréter le nom comme un protocole FFmpeg. Les appels directs de conversion publient aussi depuis un temporaire isolé et ne peuvent pas tronquer une source via une destination liée.

Le système peut répondre depuis son cache : cela ne prouve pas la conservation durable ni remplace une relecture indépendante ultérieure. C’est différent du [contrôle de conteneur](integrity.md).

<div id="what-aède-refuses-to-do" data-legacy-anchor></div>

## Refus

- Destination de base absente : un baladeur débranché ne doit pas devenir un dossier interne. Seuls les sous-dossiers d’une destination existante sont créés.
- Chevauchement avec une racine suivie, à l’intérieur, à égalité ou au-dessus, même via lien symbolique.
- Sélection vide, options contradictoires, format/qualité invalide.
- Espace prévu supérieur à celui disponible ; les tailles converties restent estimées et le disque peut encore se remplir ensuite.

Le bilan compte écrit, déjà présent et échoué. Un échec renvoie une erreur mais conserve le travail terminé. Relancer reprend ; `--replace` recommence volontairement.

<div id="transcoding-on-the-way-out" data-legacy-anchor></div>

## Convertir à la copie

```sh
aede copy /Volumes/Phone --compress opus --quality 128k
aede copy /Volumes/Phone --compress mp3 --quality V0 --query "loved"
```

Cibles : `mp3`, `opus`, `aac` dans M4A, `vorbis` dans Ogg, `flac`, `wav`. Qualités : MP3 `V0`–`V9`, Vorbis `q0`–`q10` ou débit `192k`, interprété par l’encodeur choisi. `flac`/`wav` sans perte refusent la qualité ; qualité sans `--compress` aussi.

Seules les sources identifiées sans perte sont encodées. Les sources avec perte sont copiées telles quelles, même dans un autre codec cible. MP3 vers Opus reste donc MP3 ; MP3 vers FLAC n’invente pas une qualité sans perte.

FFmpeg est requis à la conversion réelle. Conversion parallèle automatiquement ; copie simple : un traitement par défaut. `--threads N` remplace ce choix. La prévision ne lance pas l’encodage.

Les métadonnées traversent selon les limites du conteneur/FFmpeg. Images intégrées possibles dans les chemins MP3/AAC/FLAC choisis ; limitations WAV/Vorbis/Opus. WAV ne conserve pas tous les crédits/champs personnalisés. Les images annexes restent indépendamment sélectionnables.

Avec `--verify`, les lecteurs Aède vérifient la durée lisible non nulle du résultat nouvellement encodé et la comparent à la source si disponible, selon la tolérance du code. Ce n’est pas une comparaison de tous les échantillons ni une preuve de transparence audible.

<div id="a-final-note-on-data-philosophy" data-legacy-anchor></div>

### Originaux et dérivés

Les dérivés convertis peuvent recevoir les tags de l’encodeur ; jamais les originaux. Sauvegarder l’archive séparément.

Voir [copy](cli/copy.md) pour toutes les options, [backup](cli/backup.md) pour les données personnelles/catalogue sans audio.

## Reprise contrôlée et playlists

`--verify-existing` compare les copies déjà présentes avec leur source. Pour une conversion, un nouvel encodage temporaire est comparé au fichier existant : une version d’encodeur ou des métadonnées non déterministes peuvent changer les octets malgré un audio équivalent. Un écart conserve le fichier existant et signale une erreur ; `--replace` autorise explicitement sa réécriture. FFmpeg et de l’espace temporaire sont nécessaires pour comparer une conversion.

`--playlists` crée `aede-selection.m3u8` dans l’ordre de sélection et adapte les M3U/M3U8 UTF-8 copiés avec `--extras all`. Les chemins suivent les noms adaptés et les extensions converties ; les entrées hors sélection, y compris les URL, sont omises et comptées. Une playlist existante différente reste intacte sans `--replace`.

Les écritures utilisent un dossier temporaire privé créé exclusivement. Les liens symboliques dans les fichiers ou sous-dossiers de destination sont refusés. `--dry-run` ne crée aucune sonde ; sans politique explicite, les noms prévus sont conservateurs. Donner `--safe-names` ou `--raw-names` fixe la politique pour la prévision et le transfert. La conversion WAV conserve la précision entière ou flottante connue de la source ; une précision inconnue ou non prise en charge est refusée. La conversion des sources flottantes en FLAC est refusée, même en prévision : ce format entier quantifierait leurs échantillons. Un fichier déjà au format audio cible reste inchangé même si son extension est un alias. MP3 accepte V0–V9, Vorbis q0–q10, AAC/Opus uniquement un débit.

Les sources entières dépassant 24 bits exigent un encodeur FFmpeg prenant en charge le FLAC 32 bits ; un encodeur incapable est refusé. Les nouveaux fichiers sans perte sont contrôlés avant publication : précision, fréquence d’échantillonnage et nombre de canaux conservés, même sans `--verify`.
