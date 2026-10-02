# roots — Gérer les dossiers suivis

Sans argument, roots liste les dossiers suivis et exclus. Une racine est un dossier qu’Aède mémorise pour les prochains scans ; ce n’est pas le dossier de catalog.json. Retirer une racine laisse la musique sur disque.

Utiliser roots --remove DOSSIER pour cesser son suivi, roots --exclude DOSSIER pour exclure un sous-dossier, et roots --exclude DOSSIER --remove pour annuler l’exclusion. La modification déclenche normalement un scan. --no-scan l’enregistre mais laisse le catalogue actuel visible jusqu’au prochain scan. Le retrait de la dernière racine vide le catalogue sans supprimer les annotations personnelles.

La liste sans option n’ajoute aucun dossier. Pour en ajouter un, employer scan DOSSIER. Retirer un dossier non suivi ou annuler une exclusion inexistante produit une erreur.

## Syntaxe et arguments

```text
aede roots [folder…]
```

Aucun argument pour la liste. Avec --remove seul, nommer le dossier suivi. --exclude prend son dossier en valeur.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--remove` | Retirer la valeur personnelle désignée par cette commande ; sa portée est précisée ci-dessous. |
| `--exclude FOLDER_OR_ID` | Exclure le dossier indiqué (roots) ou le crédit provenant d’une source (credit), sans modifier l’audio. |
| `--no-scan` | Changer les dossiers suivis/exclus sans rescanner immédiatement. Exécuter scan ensuite pour actualiser le catalogue. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede roots
aede roots --exclude "$HOME/Music/Temporary"
aede roots --exclude "$HOME/Music/Temporary" --remove
aede roots --remove "/Volumes/Archive musicale" --no-scan
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[scan](scan.md).

Guide détaillé existant : [library.md](../../library.md).
