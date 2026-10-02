# check — Vérifier l’intégrité

check relit les sommes intégrées aux conteneurs pris en charge : CRC des trames FLAC et des pages Ogg Vorbis/Opus/Speex. Il détecte les dégâts couverts par ces sommes. MP3, MP4, WAV et AIFF ne fournissent pas de somme compatible avec cette vérification : « aucune somme » ne signifie pas « vérifié sain ».

Par défaut, les verdicts des fichiers inchangés sont réutilisés. --full relit tout le périmètre, utile après un soupçon de panne de disque. Si rien n’attend, le rapport courant s’affiche. Un dossier facultatif limite les fichiers catalogués ; scanner la nouvelle musique avant de la vérifier.

Les verdicts sont mémorisés et deviennent obsolètes si la source change. Ce contrôle diffère de la vérification MD5 du FLAC décodé ou de l’analyse d’origine audio de FlacCompagnon. Il ne répare ni ne remplace les fichiers abîmés. Sauvegarder aussi les originaux.

## Syntaxe et arguments

```text
aede check [folder…]
```

Zéro ou plusieurs dossiers contenant de l’audio catalogué. Sans argument : tout le catalogue.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--full` | Relire les sommes de tous les fichiers sélectionnés, même avec verdict existant. |
| `--threads N` | Nombre de traitements parallèles. Un entier positif fixe ce nombre ; 0 le choisit automatiquement. La copie simple utilise un traitement par défaut. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede check
aede check "$HOME/Music/Jazz" --full
aede check --threads 4
```

## Résultat et erreurs

Lire les verdicts, catégories endommagé/sans somme/illisible comprises. Le traitement check actuel peut finir avec code 0 malgré des fichiers illisibles ou verdicts endommagés : la réussite du processus ne prouve pas leur santé. Arguments invalides et erreurs de catalogue/stockage restent refusés.

## Pour continuer

[scan](scan.md), [analyze](analyze.md), [copy](copy.md).

Guide détaillé existant : [integrity.md](../../integrity.md).
