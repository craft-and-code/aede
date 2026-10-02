# love — Marquer un favori

love TYPE NOM marque une entité comme favorite. --remove retire ce marqueur en gardant étoiles, notes et étiquettes. Répéter le même état est sans effet supplémentaire.

Une requête loved hérite des favoris d’album/artiste vers les pistes ; track.loved, album.loved, artist.loved ciblent le niveau exact. Cela n’écrit pas un favori dans chaque piste. favourites liste les entités marquées ; query "loved" sélectionne des pistes lisibles.

Les annotations personnelles sont gardées séparément dans user.json, sans retaguer ni renommer la musique. Le type peut être artist, album, track, genre ou label. Reprendre l’écriture affichée et citer les noms composés ; préciser une sélection ambiguë plutôt que deviner.

## Syntaxe et arguments

```text
aede love <kind> <name>
```

Type d’entité puis son nom catalogué.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--remove` | Retirer la valeur personnelle désignée par cette commande ; sa portée est précisée ci-dessous. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede love album "Kind of Blue"
aede love artist "Miles Davis" --remove
aede query "loved played:0"
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[favourites](favourites.md), [query](query.md).

Guide détaillé existant : [annotating.md](../../annotating.md).
