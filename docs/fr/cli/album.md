# album — Explorer un album

album ouvre une parution/édition locale avec pistes ordonnées, détails techniques, tags, crédits, annotations et déclarations externes attribuées séparément. Images et liens apportent le contexte ; ouvrir la page ne retague rien.

Un identifiant MusicBrainz de parution distingue des éditions homonymes. Le groupe de parution relie l’identité d’album et les autres éditions locales. Tags œuvre/groupement/mouvement restent distincts d’une œuvre parente sourcée ; interprètes et compositeurs sont séparés.

CSV/JSON/M3U couvrent les pistes sélectionnées/paginées. --all les inclut toutes avant export. La commande singulière doit viser une parution ; l’ambiguïté est refusée avec indication, sans choisir une édition au hasard. Continue ouvre enregistrements, artistes crédités et groupe.

## Syntaxe et arguments

```text
aede album <title|MusicBrainz release ID>
```

Un titre d’album ou identifiant MusicBrainz de parution exacte.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--m3u` | Produire une playlist M3U des pistes locales sélectionnées. Elle contient des chemins, pas une copie de la musique. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede album "Kind of Blue"
aede album MUSICBRAINZ_RELEASE_ID
aede album "Kind of Blue" --all --m3u --output album.m3u8
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[albums](albums.md), [release-group](release-group.md), [track](track.md).

Guide détaillé existant : [browsing.md](../../browsing.md).
