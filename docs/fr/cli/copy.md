# copy — Copier et convertir

copy prépare une destination distincte en conservant l’arborescence source. Sans --query ni --collection, toute la bibliothèque est choisie. La destination doit déjà exister et ne chevaucher aucune racine suivie, ni à l’intérieur ni au-dessus ; les sous-dossiers exclus dans une racine restent couverts par ce contrôle. Le dossier de base n’est jamais créé : un disque débranché ne devient pas silencieusement un dossier du disque interne.

Commencer par --dry-run. Les copies simples existantes de même taille non nulle sont ignorées sans comparaison du contenu, même avec --verify ; les conversions existantes non vides aussi, sans validation complète. --replace les réécrit puis vérifie si --verify est fourni. Taille/existence ne prouvent pas leur identité. --extras cover copie seulement la pochette choisie, images toutes les images, all tous les fichiers annexes, none seulement l’audio. La détection adapte les noms refusés à destination ; --safe-names ou --raw-names remplacent ce choix.

La conversion nécessite ffmpeg. Seules les sources sans perte sont encodées ; les sources déjà avec perte sont copiées telles quelles. --quality est refusé sans --compress ou avec une cible sans perte. --verify relit les copies simples et valide les conversions ; il ne prouve pas leur égalité sonore avec la source. Les erreurs listent les échecs, les copies terminées restent présentes. Espace insuffisant et sélection vide sont signalés. Sources et tags restent intacts.

## Syntaxe et arguments

```text
aede copy <destination>
```

Exactement un dossier de destination. La sélection se donne dans --query ou --collection, pas dans des noms supplémentaires.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--threads N` | Nombre de traitements parallèles. Un entier positif fixe ce nombre ; 0 le choisit automatiquement. La copie simple utilise un traitement par défaut. |
| `--replace` | Réécrire les fichiers de destination existants ; cela ne change pas les dossiers suivis. |
| `--query EXPRESSION` | Fournir une expression relationnelle. Selon la commande : l’enregistrer, sélectionner des pistes ou garder les albums/artistes correspondants. |
| `--extras none\|cover\|images\|all` | Choisir les fichiers annexes : none, cover (défaut), images ou all. L’image intégrée est déjà dans l’audio copié. |
| `--dry-run` | Afficher le travail prévu sans télécharger ni créer de résultats. Les outils et entrées peuvent être vérifiés. |
| `--verify-existing` | Comparer les copies existantes à la source ; les conversions à un nouvel encodage temporaire. Un écart conserve le fichier et signale une erreur sans --replace. |
| `--playlists` | Créer une sélection M3U8 et adapter les M3U/M3U8 annexes aux chemins finaux ; omettre et compter les entrées hors sélection. |
| `--verify` | Relire la destination après copie. Pour une copie simple, comparer les octets avec une mémoire bornée ; après conversion, vérifier le résultat encodé et sa durée. |
| `--safe-names` | Adapter les noms aux restrictions de destination, même si la détection automatique les conserverait. |
| `--raw-names` | Conserver les caractères originaux ; les compteurs de collision restent applicables. Incompatible avec --safe-names ; des noms refusés par la destination peuvent échouer. |
| `--collection NAME` | Copier la sélection d’une collection enregistrée. Incompatible avec --query. |
| `--compress FORMAT` | Utiliser ffmpeg pour encoder les sources sans perte en mp3, opus, aac, vorbis, flac ou wav à destination. Les sources déjà avec perte restent inchangées. |
| `--quality SETTING` | Uniquement avec --compress avec perte : MP3 V0–V9, Vorbis q0–q10 ou débit tel que 192k. Inapplicable à flac/wav. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede copy "/Volumes/Player" --query "loved" --dry-run
aede copy "/Volumes/Player" --collection Road --verify
aede copy "/Volumes/Player" --query "genre:jazz" --compress mp3 --quality V0 --verify
```

## Résultat et erreurs

Sans --replace, les copies simples de taille différente et conversions vides sont conservées et signalées. La publication atomique des nouveaux fichiers exige les liens physiques, testés avant transfert, et refuse un fichier créé entre-temps. FAT/exFAT exige généralement --replace explicite, qui autorise aussi l’écrasement des fichiers existants. Une reprise déjà complète n’a pas besoin de publier. Le calcul d’espace exclut les sorties ignorées et compte les encodages temporaires ; une erreur de lecture des annexes demandées bloque le transfert.

La prévision liste pistes, annexes, octets et destination ; les adaptations de noms sont détaillées séparément. --dry-run finit après cette prévision sans écrire d’audio. Une copie réelle compte fichiers écrits, déjà présents et échoués, avec chemins et raisons des échecs. Des erreurs de fichiers renvoient un échec après conservation du travail terminé ; relancer permet de reprendre. Destination absente/chevauchante, sélection vide, conversion invalide, encodeur absent ou espace disponible insuffisant sont refusés. Les fichiers ignorés sont contrôlés seulement avec --verify-existing ; une conversion nouvellement écrite est vérifiée par sa durée lisible non nulle et sa proximité de la durée source si disponible, pas par comparaison de tous les échantillons. Sources intactes.

Les fichiers et dossiers utilisent une comparaison conservatrice des majuscules, accents composés/décomposés courants et ponctuation. Une copie réelle teste les autres équivalences sur le système de fichiers avant de modifier les résultats. Les liens symboliques de destination sont refusés ; chaque écriture utilise un dossier temporaire privé. --dry-run ne crée aucune sonde et prévoit des noms conservateurs ; --safe-names ou --raw-names fixe la politique. WAV conserve la précision entière ou flottante connue ; FLAC refuse les sources flottantes, dont les échantillons seraient quantifiés. MP3 accepte V0–V9, Vorbis q0–q10, AAC/Opus un débit uniquement. Une comparaison d’encodages exige FFmpeg et de l’espace temporaire ; elle peut différer entre versions d’encodeur ou métadonnées non déterministes.

## Pour continuer

[query](query.md), [collection](collection.md), [check](check.md).

Guide détaillé existant : [copying.md](../../copying.md).
