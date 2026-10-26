# spectrum — Créer des spectrogrammes

La version candidate de spectrum dessine des spectrogrammes à côté de la musique cataloguée avec le moteur Rust partagé de FlacCompagnon pour la FFT et le PNG. Ils montrent les fréquences dans le temps ; une coupure visuelle aide l’enquête mais ne prouve pas à elle seule un transcodage.

Le PCM natif et le profil DSF/DFF DSD64/128 mono/stéréo ne nécessitent aucun exécutable FFmpeg. Les codecs et profils FLAC 32 bits non gérés par les lecteurs natifs conservent un décodage externe optionnel ; calcul spectral et dessin restent en Rust. Ce raccordement attend une publication coordonnée des composants : les versions actuellement installées ne changent pas.

Les images sont en demi-taille par défaut ; --size full choisit la taille complète. Les images portant une empreinte de génération à jour sont conservées sans --full, même si la taille demandée a changé. Les anciennes images sans cette version sont régénérées une fois ; la fraîcheur compare la taille du fichier et sa date complète, nanosecondes comprises. --dry-run liste les résultats prévus sans dessin ; --threads règle le parallélisme.

Les valeurs invalides de `--threads` sont refusées avant tout traitement, même pour une sélection vide, des images à jour ou un aperçu `--dry-run`.

La commande crée des images annexes et exige les droits d’écriture dans ces dossiers ; les originaux restent en lecture seule. En cas d’échec, consulter le fichier signalé et vérifier format/outils avant de relancer. analyze fournit les mesures numériques ; une image n’est pas une somme d’intégrité.

Le dessin utilise une sortie temporaire isolée, puis remplace l’image finale uniquement après réussite. Les liens et fichiers spéciaux existant à destination sont refusés avant le dessin et la publication. Un échec préserve l’ancienne image ; les autres pistes sélectionnées sont traitées. Toute image en échec fait échouer la commande et sa tâche d’administration serveur, tout en conservant les images terminées.

## Syntaxe et arguments

```text
aede spectrum [folder…]
```

Dossiers catalogués facultatifs ; sans argument : tout le catalogue.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--full` | Redessiner les spectrogrammes existants, notamment après changement de taille. |
| `--size VALUE` | Avec fetch --covers : 250, 500, 1200 (défaut) ou original. Avec spectrum : half (défaut) ou full. |
| `--threads N` | Nombre de traitements parallèles. Un entier positif fixe ce nombre ; 0 le choisit automatiquement. La copie simple utilise un traitement par défaut. |
| `--dry-run` | Afficher le travail prévu sans télécharger ni créer de résultats. Les outils et entrées peuvent être vérifiés. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede spectrum "$HOME/Music/Jazz" --dry-run
aede spectrum "$HOME/Music/Jazz" --size full --full
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[analyze](analyze.md), [copy](copy.md).

Guide détaillé existant : [spectrograms.md](../../spectrograms.md).
