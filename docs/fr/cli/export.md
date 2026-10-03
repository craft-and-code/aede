# export — Exporter le catalogue et le graphe

export décrit tout le catalogue sans nom/filtre d’entité. Par défaut : JSON structurel du catalogue. --json rend ce choix explicite. --csv produit une ligne par album ; --tracks exige --csv et produit une ligne par piste.

--graph crée un fichier JSON versionné aede-graph réunissant catalogue, conclusions, sources, informations personnelles, décisions et liens attribués matérialisés. Incompatible avec CSV, il garde la provenance. Utile pour inspecter le graphe ailleurs ; backup reste le format prévu pour restaurer.

--output écrit dans le fichier choisi ; --separator concerne seulement CSV. Les exports peuvent inclure chemins absolus, textes et notes personnelles : examiner avant partage. Pour une sélection filtrée, utiliser albums/query/collection et leurs formats.

## Syntaxe et arguments

```text
aede export
```

Aucun argument positionnel ; choisir la sortie par options.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--graph` | Exporter ensemble catalogue, conclusions/preuves, décisions, données personnelles et relations attribuées. Incompatible avec CSV. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |
| `--tracks` | Uniquement avec export --csv, produire une ligne par piste au lieu d’une ligne par album. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede export --output catalog.json
aede export --csv --tracks --output tracks.csv
aede export --graph --output graph.json
```

## Résultat et erreurs

Choisir un chemin de rapport distinct des stores actifs. Les exports refusent les données/verrous actifs, l’audio existant, les liens symboliques et cibles non ordinaires avant mutation, et remplacent atomiquement les rapports ordinaires ; voir la [référence des options](options.md).

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[backup](backup.md), [query](query.md), [relations](relations.md).

Guide détaillé existant : [commands.md](../../commands.md).
