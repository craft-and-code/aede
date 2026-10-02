# rate — Attribuer des étoiles

rate TYPE NOM --stars N garde une note personnelle de 1 à 5. --remove l’efface. Les deux actions ensemble ou une note invalide/hors intervalle sont refusées.

Les étoiles ont une portée : rating:>=4 cible les pistes ; album.rating:>=4 les pistes d’un album noté. Une note d’album ne devient pas silencieusement une note de chaque piste. Lire le résultat sur album/artist/track et sélectionner ce niveau via query.

Les annotations personnelles sont gardées séparément dans user.json, sans retaguer ni renommer la musique. Le type peut être artist, album, track, genre ou label. Reprendre l’écriture affichée et citer les noms composés ; préciser une sélection ambiguë plutôt que deviner.

## Syntaxe et arguments

```text
aede rate <kind> <name>
```

Type puis nom ; fournir --stars ou --remove.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--remove` | Retirer la valeur personnelle désignée par cette commande ; sa portée est précisée ci-dessous. |
| `--stars N` | Note de 1 à 5 ; utiliser --remove pour l’effacer. Incompatible avec --remove. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede rate album "Kind of Blue" --stars 5
aede rate album "Kind of Blue" --remove
aede query "album.rating:>=4"
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[query](query.md), [album](album.md).

Guide détaillé existant : [annotating.md](../../annotating.md).
