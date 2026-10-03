# track — Explorer une piste

track cherche un titre local et montre position dans l’édition, propriétés, crédits, annotations et preuves externes. --artist/--album/--comment limitent les homonymes avant pagination. Mettre le titre d’abord : une option de nom absorbe les mots suivants jusqu’à la prochaine option.

--lyrics affiche les paroles intégrées ou .lrc sans rien télécharger. --json sépare crédits locaux, crédits sourcés d’enregistrement/œuvre, édition exacte et provenance d’œuvre parente. Les analyses importées sont attribuées, pas présentées comme fraîchement mesurées.

Le JSON est écrit dans le terminal, ou dans le fichier demandé avec `--output`, avec les mêmes champs détaillés. Tous les formats utilisent les mêmes pistes sélectionnées après filtrage et pagination.

Continue ouvre enregistrement, œuvres, album et artistes. Un enregistrement identifie une interprétation ; une piste la place dans une édition. CSV/M3U utilisent les fichiers correspondants. Ajouter artiste/album devant trop d’homonymes plutôt que supposer la première bonne édition.

## Syntaxe et arguments

```text
aede track <title>
```

Titre de piste d’abord, puis filtres de noms.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--m3u` | Produire une playlist M3U des pistes locales sélectionnées. Elle contient des chemins, pas une copie de la musique. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--artist NAME` | Filtrer par nom d’artiste avec albums/track ; avec credit, nommer la personne créditée. Les noms composés sont acceptés. |
| `--comment TEXT` | Filtrer le commentaire des fichiers. Ce sont des métadonnées intégrées, distinctes des notes personnelles. |
| `--limit N` | Afficher au plus N pistes correspondantes ; entier strictement positif. La limite par défaut est 10. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |
| `--album TITLE` | Restreindre une piste à l’album indiqué. Utile lorsque plusieurs éditions portent le même titre. |
| `--lyrics` | Avec track, afficher les paroles ; search, chercher leur texte ; fetch, télécharger les fichiers .lrc manquants depuis LRCLIB. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede track "So What" --artist "Miles Davis"
aede track "So What" --album "Kind of Blue" --lyrics
aede track "So What" --json --output track.json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[album](album.md), [recording](recording.md), [fetch](fetch.md).

Guide détaillé existant : [browsing.md](../../browsing.md).
