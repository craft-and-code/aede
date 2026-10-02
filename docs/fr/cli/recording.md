# recording — Explorer un enregistrement

recording rassemble les placements locaux partageant l’identité MusicBrainz d’enregistrement. Il montre crédits et œuvres avec source/confiance. Cela distingue une performance sur plusieurs éditions de performances différentes portant le même titre.

Employer le MBID exact pour les homonymes. Sans MBID, ouvrir local: suivi du chemin complet, comme l’exemple. Un titre identique ne fusionne pas les performances.

Continue propose des commandes copiables vers les entités voisines. Ces pages n’ont ni CSV/JSON/M3U, pagination ou --output dans la CLI actuelle ; query/export fournissent sélections/graphe structurés.

## Syntaxe et arguments

```text
aede recording <title|MusicBrainz ID>
```

Un titre ou identifiant MusicBrainz du bon type. Remplacer les exemples par ceux affichés.

## Options de cette commande

Cette commande n’a pas d’options spécifiques.

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede recording MUSICBRAINZ_RECORDING_ID
aede recording "local:/path/to/music/01.flac"
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[track](track.md), [work](work.md).

Guide détaillé existant : [commands.md](../../commands.md).
