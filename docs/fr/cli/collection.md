# collection — Enregistrer une sélection dynamique

collection NOM --query EXPRESSION enregistre une question nommée après validation syntaxique. Il garde la formule, pas les fichiers du jour. collection NOM la réévalue selon catalogue et annotations actuels.

--remove supprime seulement la définition, pas musique ni favoris. --query et --remove sont incompatibles. Sans ces options, il affiche les pistes et accepte CSV/JSON/M3U, pagination et tris de query : title, artist, album, year, duration, size, rating, played, catalog (suffixe - inverse).

collections liste les noms ; copy --collection NOM copie son résultat actuel ; play collection:NOM l’écoute localement. Un nom inconnu est refusé avec indication de création ; une définition syntaxiquement invalide n’est pas sauvée.

## Syntaxe et arguments

```text
aede collection <name>
```

Un nom de collection ; le citer s’il contient des espaces.

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
| `--sort ORDER` | Choisir une colonne disponible ; ajouter - pour décroître, par exemple duration-. Les valeurs de cette commande figurent ci-dessous. |
| `--remove` | Retirer la valeur personnelle désignée par cette commande ; sa portée est précisée ci-dessous. |
| `--query EXPRESSION` | Fournir une expression relationnelle. Selon la commande : l’enregistrer, sélectionner des pistes ou garder les albums/artistes correspondants. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede collection Road --query "loved"
aede collection Road
aede collection Road --m3u --output road.m3u8
aede collection Road --remove
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[collections](collections.md), [query](query.md), [copy](copy.md), [play](play.md).

Guide détaillé existant : [querying.md](../../querying.md).
