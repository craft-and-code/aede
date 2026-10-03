# fingerprint — Calculer une empreinte acoustique

fingerprint décode l’audio local pour calculer une empreinte Chromaprint. Il n’interroge pas AcoustID ; fetch --identify le fait séparément, permettant de reprendre le réseau sans redécoder.

Sans portée, il cible les fichiers insuffisamment identifiés par titre/artiste, plutôt que décoder toute une bibliothèque déjà taguée. Noms/dossiers sélectionnent explicitement ; --full inclut ceux identifiés/déjà calculés. --list montre les empreintes ; --dry-run prévoit sans décoder mais valide encore l’outil requis.

Les entrées doivent désigner des fichiers locaux ordinaires avant le lancement de l’outil ; périphériques, tubes et adresses de protocole sont refusés. Une durée absente, inférieure à une seconde ou hors plage dans le catalogue ne produit pas de requête. L’affichage des chemins et empreintes rend les caractères de contrôle du terminal littéralement ; les valeurs mémorisées et exportées conservent leur texte d’origine. Un aperçu ne crée ni fichier de sortie ni verrou d’écriture des données.

L’outil peut être fpcalc ou un ffmpeg avec Chromaprint ; une installation ffmpeg ordinaire ne contient pas forcément cet encodeur. La durée est requise pour AcoustID. Les résultats survivent aux scans inchangés et deviennent obsolètes si l’audio change. Les échecs sont signalés ; les réussites restent mémorisées. Une empreinte identifie un son, ce n’est pas une somme d’intégrité.

La liste des empreintes mémorisées ne prend pas non plus de verrou d’écriture et reste disponible pendant une collecte.

## Syntaxe et arguments

```text
aede fingerprint [folder…]
```

Restrictions facultatives par dossier catalogué ou nom d’artiste/album.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--list` | Lister les enregistrements ou décisions mémorisés au lieu du résumé/de l’opération habituelle. |
| `--full` | Calculer tous les fichiers sélectionnés, y compris identifiés et déjà calculés. |
| `--dry-run` | Afficher le travail prévu sans télécharger ni créer de résultats. Les outils et entrées peuvent être vérifiés. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede fingerprint "$HOME/Music/Jazz" --dry-run
aede fingerprint "$HOME/Music/Jazz"
aede fingerprint --list
aede fetch --identify "$HOME/Music/Jazz"
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[fetch](fetch.md), [check](check.md).

Guide détaillé existant : [sources.md](../../sources.md).
