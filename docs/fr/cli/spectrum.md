# spectrum — Créer des spectrogrammes

spectrum dessine des spectrogrammes à côté de la musique cataloguée via le moteur FlacCompagnon. Ils montrent les fréquences dans le temps ; une coupure visuelle aide l’enquête mais ne prouve pas à elle seule un transcodage.

Les images sont en demi-taille par défaut ; --size full choisit la taille complète. Les images existantes sont ignorées sans --full, même si la taille demandée a changé. --dry-run liste les résultats prévus sans dessin ; --threads règle le parallélisme.

La commande crée des images annexes et exige les droits d’écriture dans ces dossiers ; les originaux restent en lecture seule. En cas d’échec, consulter le fichier signalé et vérifier format/outils avant de relancer. analyze fournit les mesures numériques ; une image n’est pas une somme d’intégrité.

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
