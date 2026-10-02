# cancel — Arrêter une tâche déléguée

cancel demande l’arrêt d’un scan ou fetch délégué au serveur Unix local. Copier l’identifiant affiché par la CLI initiale et reprendre le même compte et dossier de données. Cet identifiant désigne un travail, pas un album/fichier.

La demande revient immédiatement ; la CLI initiale quitte avec le code 130 une fois arrêtée. Les réponses fetch déjà enregistrées restent présentes. Fermer la CLI ou utiliser son Ctrl-C n’annule pas une tâche déléguée. Sans serveur, cancel refuse ; une commande exécutée localement s’arrête avec Ctrl-C.

Les tâches terminées, celles d’un serveur redémarré et les commandes déléguées autres que scan/fetch sont refusées. Les tâches HTTP administratives utilisent leur route d’annulation authentifiée, pas cette commande. Annulation/délégation locale indisponibles sous Windows.

## Syntaxe et arguments

```text
aede cancel <task-id>
```

Exactement un identifiant affiché par scan/fetch délégué. Remplacer TASK_ID, valeur d’exemple.

## Options de cette commande

Cette commande n’a pas d’options spécifiques.

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede cancel TASK_ID --data "/path/to/aede-data"
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[serve](serve.md), [scan](scan.md), [fetch](fetch.md).

Guide détaillé existant : [operating.md](../../operating.md).
