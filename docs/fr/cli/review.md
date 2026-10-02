# review — Examiner les identités proposées

review liste correspondances de noms incertaines et conflits entre identités de sources et tags. Chaque proposition possède un identifiant stable. Comparer les faits avant d’accepter : cela autorise précisément cette preuve dans navigation et requêtes ; un refus la garde seulement comme preuve.

La liste normale montre les cas en attente. --all inclut les décisions résolues. --interactive présente les cartes contextuelles dans le terminal. --accept ID, --reject ID et --undo ID sont des actions distinctes : elles refusent filtre de nom, pagination/source et mode interactif. --undo remet en attente.

Aucun tag n’est réécrit. Confiance de source et décision personnelle restent séparées : accepter ne crée pas un identifiant lu dans l’audio. Les décisions restent dans sources.json et passent par rules export/import. Les identifiants invalides, ambigus ou absents sont refusés. doctor aide à trouver les cas à traiter.

## Syntaxe et arguments

```text
aede review [name] [--interactive] | aede review --accept=<ID> | --reject=<ID> | --undo=<ID>
```

Filtre de nom facultatif pour la liste seulement ; décisions directes avec les identifiants affichés. Remplacer CLAIM_ID.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--source NAME` | Filtrer selon le nom de source/d’outil mémorisé. Employer les noms affichés dans la liste correspondante. |
| `--accept ID` | Accepter précisément cette proposition pour la navigation et les requêtes, en conservant sa preuve de source. |
| `--reject ID` | Conserver la proposition comme preuve mais empêcher son usage comme relation de confiance dans navigation/requêtes. |
| `--undo ID` | Annuler cette décision (review) ou restaurer ce crédit de source exclu (credit). |
| `--interactive` | Examiner chaque proposition avec son contexte dans le terminal. Incompatible avec --accept, --reject et --undo. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Inclure décisions résolues et propositions en attente, sans limite de lignes. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede review
aede review --interactive
aede review --accept CLAIM_ID
aede review --undo CLAIM_ID
aede review --all
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[sources](sources.md), [doctor](doctor.md), [rules](rules.md).

Guide détaillé existant : [sources.md](../../sources.md).
