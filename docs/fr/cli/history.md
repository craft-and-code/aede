# history — Consulter l’historique

history affiche les écoutes de la plus récente à la plus ancienne, avec résumé/compteurs. play enregistre automatiquement ; played ajoute une écoute extérieure. La pagination règle l’affichage, pas les enregistrements mémorisés.

--remove demande l’effacement de tout l’historique, pas seulement la page visible. --yes évite la confirmation. played TITRE --remove retire plutôt la dernière écoute d’une seule piste. Effacer l’historique ne supprime ni musique ni notes.

La CLI actuelle n’exporte pas history en CSV/JSON/--output. Utiliser backup pour le préserver. Données personnelles dans user.json ; rules les exclut volontairement.

## Syntaxe et arguments

```text
aede history
```

Aucun argument positionnel.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--yes` | Accepter la confirmation demandée par cette commande. Vérifier l’opération auparavant ; réserver aux exécutions automatisées maîtrisées. |
| `--remove` | Effacer tout l’historique après confirmation, pas seulement la page visible. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede history --limit 20
aede history --offset 20 --limit 20
aede history --remove
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[played](played.md), [play](play.md), [backup](backup.md).

Guide détaillé existant : [annotating.md](../../annotating.md).
