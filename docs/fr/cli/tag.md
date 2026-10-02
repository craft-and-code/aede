# tag — Ajouter des étiquettes personnelles

tag TYPE NOM ÉTIQUETTE[,ÉTIQUETTE] ajoute des catégories personnelles. Citer le nom complet afin que le dernier argument désigne clairement les étiquettes. Elles diffèrent des tags audio album/titre.

--remove avec étiquettes retire seulement celles nommées. Sans étiquette, --remove les efface toutes sur cette entité. Citer le nom surtout pour un titre composé. Étoiles, notes et favoris restent présents.

query tag:ÉTIQUETTE cible les pistes ; album.tag:ÉTIQUETTE ou artist.tag:ÉTIQUETTE changent le niveau. Aucun vocabulaire externe n’est imposé.

Les annotations personnelles sont gardées séparément dans user.json, sans retaguer ni renommer la musique. Le type peut être artist, album, track, genre ou label. Reprendre l’écriture affichée et citer les noms composés ; préciser une sélection ambiguë plutôt que deviner.

## Syntaxe et arguments

```text
aede tag <kind> <name> <label[,label…]>
```

Type, nom cité, puis étiquettes séparées par virgules ; facultatives seulement pour tout retirer.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--remove` | Retirer les étiquettes nommées ou toutes si aucune n’est nommée. Les autres annotations restent. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede tag album "Kind of Blue" evening,jazz
aede tag album "Kind of Blue" evening --remove
aede tag album "Kind of Blue" --remove
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[query](query.md), [note](note.md).

Guide détaillé existant : [annotating.md](../../annotating.md).
