# note — Écrire des notes Markdown

note TYPE NOM lit la note actuelle sans option d’écriture/retrait. Une note par entité. --text fournit un texte court, --file lit un fichier UTF-8, --file - lit l’entrée d’un programme précédent. Choisir une source ; --from TYPE:NOM copie une autre note.

Le nouveau texte remplace la note sauf --append, qui l’ajoute après une ligne vide. --remove efface et refuse les actions d’écriture contradictoires. Markdown, lignes vides et formulation sont préservés ; le terminal montre le texte source et un client graphique peut le rendre de façon sûre.

Exporter avec notes --export et sauvegarder user.json via backup. search --notes ou query album.note:TEXTE retrouvent la prose. Une note personnelle diffère du commentaire intégré à l’audio.

Les annotations personnelles sont gardées séparément dans user.json, sans retaguer ni renommer la musique. Le type peut être artist, album, track, genre ou label. Reprendre l’écriture affichée et citer les noms composés ; préciser une sélection ambiguë plutôt que deviner.

## Syntaxe et arguments

```text
aede note <kind> <name>
```

Type puis nom ; --from désigne une autre entité par TYPE:NOM.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--remove` | Retirer la valeur personnelle désignée par cette commande ; sa portée est précisée ci-dessous. |
| `--text TEXT` | Écrire le texte de la note personnelle. Les mots sont consommés jusqu’à la prochaine option. |
| `--file FILE_OR_-` | Lire une note UTF-8 depuis FICHIER. Le caractère - seul lit l’entrée standard d’un programme précédent. |
| `--append` | Ajouter le texte fourni à la note existante, séparé par une ligne vide. Sans cette option, remplacer la note. |
| `--from KIND:NAME` | Copier une note depuis une autre référence, telle que album:"Kind of Blue". Incompatible avec une autre source de texte. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede note album "Kind of Blue" --file kind-of-blue.md
aede note album "Kind of Blue"
aede note album "Kind of Blue" --text "A new observation" --append
aede note album "Kind of Blue" --remove
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[notes](notes.md), [search](search.md), [backup](backup.md).

Guide détaillé existant : [annotating.md](../../annotating.md).
