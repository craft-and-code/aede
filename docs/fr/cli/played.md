# played — Noter une écoute extérieure

played ajoute une écoute manuelle d’une piste cataloguée entendue ailleurs. Utile si un autre lecteur ne met pas à jour cet historique. play enregistre déjà ses écoutes ; ne pas les ajouter une seconde fois.

--remove retire la dernière écoute de cette piste, pas tout l’historique. Lire history ensuite ou query played:0 pour les pistes sans écoute enregistrée. Le compteur est un relevé personnel, pas toutes les écoutes sur des services externes.

Une piste inconnue/ambiguë doit être précisée. L’historique est dans user.json et backup, mais exclu du fichier rules plus limité.

## Syntaxe et arguments

```text
aede played <track>
```

Un titre de piste cataloguée ou une référence exacte `track:CHEMIN`. Les références à un album, un artiste ou un autre type d’entité sont refusées.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--remove` | Retirer la valeur personnelle désignée par cette commande ; sa portée est précisée ci-dessous. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede played "So What"
aede played "So What" --remove
aede history
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[history](history.md), [play](play.md), [query](query.md).

Guide détaillé existant : [annotating.md](../../annotating.md).
