# albums — Lister et filtrer les albums

albums liste les parutions/éditions locales. Combiner --artist, --year, --genre, --label, --comment et compilation par ET. --query garde les albums avec au moins une piste correspondante ; une annotation d’album exige album.rating/album.tag dans l’expression. Tris : name/title, artist, year, tracks, duration/length, size.

Cette commande de liste n’accepte pas de nom d’entité en argument. Ouvrir une entité via sa commande de détail, pas en ajoutant son nom à la liste. L’ordre est déterministe. Si disponible, --offset compte les lignes sautées depuis zéro, --limit est positif, --all est incompatible avec --limit. CSV/JSON exportent le résultat filtré/paginé, pas automatiquement toute la bibliothèque. Ajouter --all si disponible pour tout exporter. La liste reste en lecture seule.

## Syntaxe et arguments

```text
aede albums
```

Aucun argument positionnel. Employer les filtres ci-dessous.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--compilations` | Lister seulement les compilations. Incompatible avec --no-compilations. |
| `--no-compilations` | Exclure les compilations. Incompatible avec --compilations. |
| `--artist NAME` | Filtrer par nom d’artiste avec albums/track ; avec credit, nommer la personne créditée. Les noms composés sont acceptés. |
| `--year YYYY` | Filtrer les albums selon une année numérique, pas un intervalle. Utiliser query pour un intervalle. |
| `--genre NAME` | Garder les albums correspondant à ce genre. Utiliser query pour les OU, exclusions et critères combinés. |
| `--label NAME` | Garder les albums correspondant à ce label discographique. |
| `--comment TEXT` | Filtrer le commentaire des fichiers. Ce sont des métadonnées intégrées, distinctes des notes personnelles. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |
| `--sort ORDER` | Choisir une colonne disponible ; ajouter - pour décroître, par exemple duration-. Les valeurs de cette commande figurent ci-dessous. |
| `--query EXPRESSION` | Fournir une expression relationnelle. Selon la commande : l’enregistrer, sélectionner des pistes ou garder les albums/artistes correspondants. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede albums --artist "Miles Davis"
aede albums --year 1959 --all
aede albums --query "album.rating:>=4"
aede albums --csv --output albums.csv
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[album](album.md), [query](query.md).

Guide détaillé existant : [browsing.md](../../browsing.md).
