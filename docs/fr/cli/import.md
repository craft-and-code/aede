# import — Importer des analyses

import lit les rapports JSON [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/), y compris ceux d’artiste couvrant plusieurs albums. Il peut précéder le premier scan : les mesures se rattachent par chemin et attendent que la musique correspondante soit cataloguée. Il ne relance ni analyse acoustique ni réseau.

--list distingue résultats rattachés, en attente et obsolètes. --pending garde ceux sans fichier encore catalogué. --forget retire les analyses mémorisées sélectionnées ; avec --pending, seulement celles en attente. Les dossiers limitent --list, --pending et --forget --pending ; --forget seul refuse un dossier. L’import normal accepte des rapports ou des dossiers de rapports parcourus récursivement. --source limite à un outil.

La date de rapport la plus récente gagne lors d’un recouvrement ; changer l’audio rend la mesure obsolète. Supprimer le rapport ensuite n’efface pas l’import. JSON mal formé, rapport incompatible ou chemin illisible sont signalés. analyze produit de nouvelles mesures dans Aède.

## Syntaxe et arguments

```text
aede import <report…>
```

Mode normal : rapports JSON ou dossiers parcourus récursivement. Liste/attente/retrait en attente : dossiers facultatifs ; --forget seul sans dossier.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--forget` | Retirer les données/décisions mémorisées dans cette portée. Aucun fichier audio n’est supprimé ni retagué. |
| `--pending` | Limiter les analyses importées à celles en attente d’un fichier scanné correspondant. Avec --forget, retirer seulement ces résultats. |
| `--list` | Lister les enregistrements ou décisions mémorisés au lieu du résumé/de l’opération habituelle. |
| `--source NAME` | Filtrer selon le nom de source/d’outil mémorisé. Employer les noms affichés dans la liste correspondante. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede import "/path/to/artist-report.json"
aede import --list --all
aede import --pending
aede import --forget --pending
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[analyze](analyze.md), [scan](scan.md), [track](track.md).

Guide détaillé existant : [imported-analyses.md](../../imported-analyses.md).
