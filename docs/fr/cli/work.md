# work — Explorer une œuvre

work ouvre une identité de composition et ses enregistrements. Compositeurs, paroliers, auteurs et arrangeurs sourcés restent distincts des interprètes d’un enregistrement. Les relations partie-d’œuvre explicites permettent navigation parent/mouvements.

Une œuvre présente seulement par récupération est identifiée comme preuve externe. Le partage d’un tag WORK ne crée pas d’identité commune canonique. query work:ID_PARENT sélectionne les parties identifiées ; les textes locaux restent cherchables sans inventer des relations.

Continue propose des commandes copiables vers les entités voisines. Ces pages n’ont ni CSV/JSON/M3U, pagination ou --output dans la CLI actuelle ; query/export fournissent sélections/graphe structurés.

## Syntaxe et arguments

```text
aede work <title|MusicBrainz ID>
```

Un titre ou identifiant MusicBrainz du bon type. Remplacer les exemples par ceux affichés.

## Options de cette commande

Cette commande n’a pas d’options spécifiques.

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede work MUSICBRAINZ_WORK_ID
aede query "work:MUSICBRAINZ_WORK_ID"
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[recording](recording.md), [query](query.md).

Guide détaillé existant : [commands.md](../../commands.md).
