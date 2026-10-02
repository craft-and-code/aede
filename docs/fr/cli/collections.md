# collections — Lister les collections

collections liste les requêtes dynamiques enregistrées et le nombre de pistes qu’elles sélectionnent maintenant. C’est un aperçu des définitions, pas un magasin de playlists statiques. Une liste vide signifie qu’aucune définition n’est sauvée ; la création est indiquée.

Ouvrir avec collection NOM, modifier avec --query, retirer avec --remove sur la commande singulière. collections au pluriel n’accepte ni nom, ni pagination, ni format d’export. Il ne modifie aucune définition et ne copie pas l’audio.

## Syntaxe et arguments

```text
aede collections
```

Aucun argument positionnel.

## Options de cette commande

Cette commande n’a pas d’options spécifiques.

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede collections
aede collection Road
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[collection](collection.md).

Guide détaillé existant : [querying.md](../../querying.md).
