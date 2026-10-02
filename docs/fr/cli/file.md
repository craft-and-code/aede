# file — Inspecter un fichier directement

file lit directement un chemin audio existant, sans catalogue. Il affiche conteneur/codec, durée, caractéristiques et tags lisibles. Utile avant scan ou pour comprendre une divergence de tag/lecture.

C’est une inspection, pas une analyse audio décodée complète ni preuve d’intégrité. Les formats lisibles en tags peuvent différer du décodage natif de lecture. Fichier illisible/corrompu/non pris en charge : diagnostic, pas réparation.

Aucun CSV/JSON/--output pour cette page humaine. Elle lit seulement sans retaguer. track après scan ajoute annotations, liens et mesures importées.

## Syntaxe et arguments

```text
aede file <path>
```

Un chemin de fichier audio existant ; citer les espaces.

## Options de cette commande

Cette commande n’a pas d’options spécifiques.

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede file "/path/to/music/01.flac"
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[scan](scan.md), [track](track.md), [check](check.md).

Guide détaillé existant : [formats.md](../../formats.md).
