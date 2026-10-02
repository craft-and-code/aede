# release-group — Relier les éditions d’un album

release-group ouvre l’identité d’album commune aux éditions locales. Il liste les parutions/placements et leurs pages exactes, plutôt que traiter deux pressages comme deux albums sans lien.

Le MusicBrainz release-group identifie l’album commun ; release identifie une édition exacte. Employer ces identifiants distincts pour les homonymes. La page décrit seulement le graphe mémorisé sans réseau ni réécriture.

Continue propose des commandes copiables vers les entités voisines. Ces pages n’ont ni CSV/JSON/M3U, pagination ou --output dans la CLI actuelle ; query/export fournissent sélections/graphe structurés.

## Syntaxe et arguments

```text
aede release-group <title|MusicBrainz ID>
```

Un titre ou identifiant MusicBrainz du bon type. Remplacer les exemples par ceux affichés.

## Options de cette commande

Cette commande n’a pas d’options spécifiques.

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede release-group MUSICBRAINZ_RELEASE_GROUP_ID
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[album](album.md), [albums](albums.md).

Guide détaillé existant : [commands.md](../../commands.md).
