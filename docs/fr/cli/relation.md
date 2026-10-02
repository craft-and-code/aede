# relation — Annoter une relation

relation ouvre l’identifiant exact copié dans relations et montre extrémités et provenance. --text écrit une note personnelle ; --tag ajoute des étiquettes séparées par virgules. Ces annotations restent dans user.json et ne changent ni extrémités ni assertion de source.

--remove seul retire toute l’annotation. --tag ÉTIQUETTE --remove retire seulement ces étiquettes. Mélanger nouvelle note et retrait est refusé. Si le lien a disparu, l’identifiant de son annotation conservée reste utilisable pour la supprimer.

C’est l’endroit pour « vérifier dans le livret » ou « collaboration intéressante ». Cela n’édite pas le graphe : credit corrige/exclut un crédit de source, review choisit la confiance d’une identité proposée.

## Syntaxe et arguments

```text
aede relation <ID>
```

Exactement un identifiant stable de relation. Remplacer RELATION_ID.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--remove` | Retirer toute l’annotation personnelle du lien ou seulement --tag si fourni ; ne jamais supprimer le lien. |
| `--text TEXT` | Écrire le texte de la note personnelle. Les mots sont consommés jusqu’à la prochaine option. |
| `--tag LABEL[,LABEL]` | Avec notes/relations, filtrer une étiquette personnelle ; avec relation, ajouter les étiquettes séparées par des virgules, ou les retirer avec --remove. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede relation RELATION_ID
aede relation RELATION_ID --text "Verify against booklet" --tag dubious,booklet
aede relation RELATION_ID --tag dubious --remove
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[relations](relations.md), [credit](credit.md), [review](review.md).

Guide détaillé existant : [commands.md](../../commands.md).
