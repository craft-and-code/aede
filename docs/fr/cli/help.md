# help — Trouver l’aide hors ligne

help montre l’index sans nom. help COMMANDE et COMMANDE --help ouvrent la même page spécialisée, aliases compris. Cette aide est intégrée à l’exécutable et fonctionne hors ligne.

--version (aussi -v/-V) identifie la version pour un signalement. --no-color désactive les couleurs et facilite la copie. Options inconnues/valeurs manquantes sont validées avant aide/version ; une faute peut donc être refusée plutôt qu’ignorée.

Le site développe cette aide courte par exemples et contexte. La référence des options explique syntaxe/portée : --json et --limit n’appartiennent qu’à certaines commandes.

## Syntaxe et arguments

```text
aede help [command]
```

Nom de commande ou alias facultatif ; un seul sujet.

## Options de cette commande

Cette commande n’a pas d’options spécifiques.

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede help
aede help fetch
aede fetch --help
aede --version
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[scan](scan.md).

Guide détaillé existant : [commands.md](../../commands.md).
