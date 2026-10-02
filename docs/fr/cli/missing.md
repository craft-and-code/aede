# missing — Repérer les albums manquants

missing compare les discographies MusicBrainz de confiance déjà récupérées à la collection locale. Lancer d’abord fetch --discography. Sans ces preuves, il explique que la comparaison est indisponible plutôt que prétendre la collection complète.

Normalement, il liste les albums studio absents et laisse de côté singles, concerts, compilations/démos et vos exclusions. --all lève ces exclusions et la pagination, avec leur raison. C’est une comparaison de métadonnées, pas un gestionnaire d’achats, possession ou prêts ; identification d’édition et discographies incomplètes peuvent affecter le résultat.

--forget TITRE met une parution de côté sans effacer la source. --list montre ces décisions. --forget --remove TITRE la remet. Plusieurs parutions également correspondantes sont refusées pour éviter de masquer le mauvais album. L’option --source acceptée n’est actuellement pas appliquée ; donner un nom précis et lire la portée affichée.

## Syntaxe et arguments

```text
aede missing [name…]
```

Noms d’artistes/albums facultatifs ; la mise de côté exige une parution manquante non ambiguë.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--forget` | Retirer les données/décisions mémorisées dans cette portée. Aucun fichier audio n’est supprimé ni retagué. |
| `--list` | Lister les enregistrements ou décisions mémorisés au lieu du résumé/de l’opération habituelle. |
| `--source NAME` | Reconnu par la portée, mais le traitement missing actuel n’applique pas ce filtre. Ne pas compter dessus pour limiter le rapport. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Inclure les parutions hors studio et mises de côté ainsi que toutes les lignes ; préciser leur exclusion habituelle. |
| `--remove` | Retirer la valeur personnelle désignée par cette commande ; sa portée est précisée ci-dessous. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede fetch --discography "Miles Davis"
aede missing "Miles Davis"
aede missing --forget "A missing album"
aede missing --list
aede missing --forget --remove "A missing album"
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[fetch](fetch.md), [artist](artist.md), [rules](rules.md).

Guide détaillé existant : [sources.md](../../sources.md).
