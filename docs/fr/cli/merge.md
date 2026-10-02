# merge — Rapprocher deux noms d’artiste

merge enregistre votre décision qu’une écriture de nom doit être classée sous une autre. C’est une règle personnelle dans user.json, sans réécrire tags ni métadonnées externes. La première écriture cède à la seconde ; citer séparément chaque nom complet.

Le prochain scan reconstruit le graphe selon la règle. --list affiche les règles et leur prise d’effet. --forget PREMIER retire une règle ; refaire un scan pour annuler son effet. Des noms encore absents peuvent être déclarés avant un scan, avec avertissement révélant une faute possible.

Deux identifiants MusicBrainz locaux différents provoquent un refus : ils désignent des personnes distinctes, à corriger volontairement à la source. doctor suggère des rapprochements sans les appliquer. --source ne crée pas un mode de fusion approximative automatique.

## Syntaxe et arguments

```text
aede merge <first> <second>
```

Action normale : exactement deux écritures citées séparément. Retrait : écriture dont la règle doit être oubliée.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--forget` | Retirer les données/décisions mémorisées dans cette portée. Aucun fichier audio n’est supprimé ni retagué. |
| `--list` | Lister les enregistrements ou décisions mémorisés au lieu du résumé/de l’opération habituelle. |
| `--source NAME` | Reconnu par la portée, mais le traitement merge actuel n’applique pas ce filtre. Les règles restent personnelles. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede merge "O. Osbourne" "Ozzy Osbourne"
aede scan
aede merge --list
aede merge --forget "O. Osbourne"
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[scan](scan.md), [doctor](doctor.md), [rules](rules.md).

Guide détaillé existant : [library.md](../../library.md).
