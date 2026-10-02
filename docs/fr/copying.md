
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

Aède teste la destination pour ses restrictions et peut adapter caractères, fins de noms et noms de périphériques réservés. `--safe-names` force l’adaptation ; `--raw-names` conserve exactement les noms et peut échouer. Options incompatibles.

Les noms adaptés sont listés. Des compteurs déterministes distinguent les collisions, évitant l’écrasement de deux pistes sélectionnées.

<div id="ensuring-it-arrives-intact" data-legacy-anchor></div>

## Vérifier l’arrivée

Un nouveau fichier est écrit sous un nom temporaire avant publication. Les copies simples déjà présentes à la taille attendue non nulle sont ignorées sans comparaison de contenu. Une conversion existante non vide l’est aussi. Taille/existence servent à reprendre, pas à prouver l’identité.

`--verify` relit les copies simples **nouvellement écrites** et compare leur CRC-32 à la source après vidange applicable. Les fichiers ignorés ne sont pas relus. `--replace --verify` les réécrit et vérifie volontairement.

Le système peut répondre depuis son cache : cela ne prouve pas la conservation durable ni remplace une relecture indépendante ultérieure. CRC-32 concerne les dégâts accidentels, pas une collision volontaire. C’est différent du [contrôle de conteneur](integrity.md).

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
