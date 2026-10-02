# extract — Extraire les images intégrées

extract écrit à côté de la musique les images déjà intégrées aux tags pris en charge. Il reste local, sans Cover Art Archive ni Fanart.tv. Le lancer avant fetch --covers si les fichiers contiennent une bonne pochette.

Par défaut, il extrait la pochette sélectionnée. --images conserve aussi dos/livrets/disques intégrés dans artwork/. --dry-run affiche le travail sans écrire. Les images locales existantes ne sont pas remplacées.

Il exige les droits d’écriture à côté de l’album mais lit seulement l’audio. Sans image intégrée, aucune image n’est produite ; une pochette peut ensuite être téléchargée. Rescanner pour actualiser le choix d’image du catalogue. artwork est un alias exact.

Alias : `aede artwork`. Options et comportement identiques.

## Syntaxe et arguments

```text
aede extract [folder…]
```

Dossiers de musique cataloguée facultatifs.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--images` | Garder les images annexes intégrées/téléchargées, par exemple dos et livrets, dans artwork/. |
| `--dry-run` | Afficher le travail prévu sans télécharger ni créer de résultats. Les outils et entrées peuvent être vérifiés. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede extract "$HOME/Music/Jazz" --dry-run
aede extract "$HOME/Music/Jazz" --images
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[fetch](fetch.md), [scan](scan.md).

Guide détaillé existant : [library.md](../../library.md).
