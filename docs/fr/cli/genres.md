# genres — Parcourir les genres

genres regroupe les genres des tags et leurs totaux de pistes/durée/taille. Il ne déduit pas le genre du son et ne remplace pas les tags par une classification publique. Tris : name/title, tracks, duration/length, size. genre NOM ouvre un groupe.

Cette commande de liste n’accepte pas de nom d’entité en argument. Ouvrir une entité via sa commande de détail, pas en ajoutant son nom à la liste. L’ordre est déterministe. Si disponible, --offset compte les lignes sautées depuis zéro, --limit est positif, --all est incompatible avec --limit. CSV/JSON exportent le résultat filtré/paginé, pas automatiquement toute la bibliothèque. Ajouter --all si disponible pour tout exporter. La liste reste en lecture seule.

## Syntaxe et arguments

```text
aede genres
```

Aucun argument positionnel. Employer les filtres ci-dessous.

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
| `--sort ORDER` | Choisir une colonne disponible ; ajouter - pour décroître, par exemple duration-. Les valeurs de cette commande figurent ci-dessous. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede genres
aede genres --sort tracks- --limit 10
aede genres --json --output genres.json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[genre](genre.md), [query](query.md).

Guide détaillé existant : [browsing.md](../../browsing.md).
