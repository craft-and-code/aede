# restore — Restaurer une sauvegarde

restore lit une sauvegarde Aède et présente chaque magasin qu’il peut remplacer avant confirmation. Un magasin lisible présent remplace son équivalent actuel. Un magasin absent ou non pris en charge reste intact, jamais supprimé. C’est un remplacement, contrairement aux imports notes/rules qui fusionnent.

Arrêter le serveur pour une récupération réelle. Sauvegarder les données actuelles avant pour revenir en arrière. Pour un essai, utiliser un dossier --data vide distinct et examiner son contenu avant la production. --yes saute volontairement la confirmation.

Le catalogue est une photographie de la date de sauvegarde. Exécuter scan ensuite pour rapprocher musique ajoutée/retirée, toutes les racines accessibles. restore ne récupère pas l’audio absent, non inclus. Un fichier illisible ou sans magasin restaurable est refusé.

## Syntaxe et arguments

```text
aede restore <file>
```

Un fichier de sauvegarde Aède existant ; pas un dossier musical, CSV ou rapport FlacCompagnon.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--yes` | Accepter la confirmation demandée par cette commande. Vérifier l’opération auparavant ; réserver aux exécutions automatisées maîtrisées. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede restore aede-backup.aede --data "/path/to/rehearsal-data"
aede restore aede-backup.aede
```

## Résultat et erreurs

Avant confirmation, chaque magasin indique remplacement des données actuelles, création ou conservation sans changement. Le bilan compte magasins restaurés, date du catalogue sauvegardé et racines absentes sur cette machine. Monter ces racines avant de scanner. Refuser la confirmation renvoie “nothing was restored” ; bundle illisible ou sans magasin compatible : erreur. Les magasins sont écrits séparément : un échec d’écriture peut laisser une restauration partielle. Sauvegarder les données actuelles et arrêter les écritures concurrentes avant ; ce n’est pas une transaction unique pour tous les magasins.

## Pour continuer

[backup](backup.md), [scan](scan.md).

Guide détaillé existant : [operating.md](../../operating.md).
