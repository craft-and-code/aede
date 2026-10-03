# extract — Extraire les images intégrées

extract écrit à côté de la musique les images déjà intégrées aux tags pris en charge. Il reste local, sans Cover Art Archive ni Fanart.tv. Le lancer avant fetch --covers si les fichiers contiennent une bonne pochette.

Par défaut, il extrait la pochette sélectionnée. --images conserve aussi dos/livrets/disques intégrés dans artwork/. --dry-run affiche le travail sans écrire ni créer le verrou d’écriture. Les images locales existantes ne sont pas remplacées. L’extraction regroupe une fois le catalogue par dossier et lit le premier fichier contenant des images dans chacun, en gardant l’ordre du catalogue : elle ne reparcourt pas toutes les pistes pour trouver cette source.

Il exige les droits d’écriture à côté de l’album mais lit seulement l’audio. Publier une nouvelle image exige un système de fichiers acceptant les liens physiques ; un système incompatible refuse la publication plutôt que risquer un remplacement. Les pixels JPEG et PNG statiques sont décodés avant publication, avec la même validation que [fetch](fetch.md) : fin du conteneur, 32 Mio en entrée, 8192 pixels par axe et 16 millions de pixels. Les images non décodables et les PNG animés sont refusés avant toute création de fichier. Aucun outil externe n’est nécessaire. Sans image intégrée, aucune image n’est produite ; une pochette peut ensuite être téléchargée. Rescanner pour actualiser le choix d’image du catalogue. artwork est un alias exact.

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
