# reset — Reconstruire le catalogue

reset supprime catalog.json après présentation de son contenu et confirmation. Il ne supprime ni musique, ni sources.json, user.json ou conclusions.json. Notes, étoiles, favoris, empreintes, verdicts et analyses importées gardés séparément subsistent.

Les dossiers suivis vivent dans catalog.json : il faut les nommer à nouveau lors de la reconstruction. La commande affiche un scan avec les anciennes racines. Le copier avant de fermer le terminal. Malgré un ancien résumé d’aide, reset ne conserve pas ces racines dans un catalogue de remplacement.

Faire une sauvegarde auparavant pour récupérer exactement l’état du catalogue. --yes accepte la suppression sans question ; cela ne la rend pas réversible. Sans catalogue, reset indique qu’il n’y a rien à retirer.

## Syntaxe et arguments

```text
aede reset
```

Aucun argument positionnel.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--yes` | Accepter la confirmation demandée par cette commande. Vérifier l’opération auparavant ; réserver aux exécutions automatisées maîtrisées. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede backup before-reset.aede
aede reset
aede scan "$HOME/Music"
```

## Résultat et erreurs

Avant confirmation, le tableau indique pistes, albums, artistes, racines suivies et fichier de catalogue à supprimer. Après suppression, “catalog removed” est suivi d’un scan avec les anciennes racines pour reconstruire. Répondre non affiche “nothing was removed” et réussit sans suppression ; un catalogue absent réussit aussi avec explication. Un catalogue présent illisible ou une suppression impossible renvoie une erreur. Reconstruire exige des racines explicites et peut nécessiter de rétablir les exclusions ; restaurer une sauvegarde retrouve la configuration exacte.

## Pour continuer

[backup](backup.md), [scan](scan.md).

Guide détaillé existant : [library.md](../../library.md).
