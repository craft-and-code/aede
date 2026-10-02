# favourites — Retrouver les favoris

favourites liste les entités explicitement marquées par love : artistes, albums, pistes et autres types annotables. C’est une liste d’entités, pas la playlist héritée de toutes leurs pistes. favorites est un alias identique.

query "loved" donne la sélection de pistes héritée ; query "track.loved" les pistes directement marquées. Exporter en CSV/JSON avec --output ; M3U est indisponible car les lignes ne sont pas toutes des pistes. La pagination précède l’export. Retirer avec love TYPE NOM --remove, pas cette lecture seule.

Alias : `aede favorites`. Options et comportement identiques.

## Syntaxe et arguments

```text
aede favourites
```

Aucun argument positionnel.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede favourites
aede favorites --all --csv --output favourites.csv
aede query "loved" --m3u
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[love](love.md), [query](query.md).

Guide détaillé existant : [annotating.md](../../annotating.md).
