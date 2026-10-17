# relation — Annoter une relation

relation ouvre l’identifiant exact copié dans relations et montre extrémités et provenance. --text écrit une note personnelle ; --tag ajoute des étiquettes séparées par virgules. Ces annotations restent dans user.json et ne changent ni extrémités ni assertion de source.

Les métadonnées de source et le contexte personnel sont affichés littéralement, y compris les caractères de contrôle du terminal. Le stockage et les exports JSON du graphe conservent les valeurs originales.

--remove seul retire toute l’annotation. --tag ÉTIQUETTE --remove retire seulement ces étiquettes. Mélanger nouvelle note et retrait est refusé. Si le lien a disparu, l’identifiant de son annotation conservée reste utilisable pour la supprimer.

Un préfixe d’identifiant correspondant à plusieurs liens actuels est ambigu et refusé pour la suppression comme pour la consultation. Une sélection actuelle ambiguë n’est jamais assimilée à un lien disparu.

Les notes et étiquettes d’un crédit restent attachées lorsque le même artiste MusicBrainz exact passe d’une extrémité connue par la source à un artiste du catalogue local. Cela concerne liste, filtre, consultation, modification, suppression, export du graphe et diagnostic des annotations orphelines ; les identifiants du graphe et les références sauvegardées ne sont pas réécrits. La correspondance exacte du propriétaire et de la référence est prioritaire. Seul un crédit équivalent unique, prouvé par le catalogue et gardant le même rôle, la même provenance et le même identifiant de relation source, sert de repli. Les annotations historiques ambiguës sont conservées et leur modification refusée plutôt que fusionnées. Une nouvelle annotation de crédit utilise le MBID d’artiste connu uniquement si cette référence désigne de manière unique le même artiste local sélectionné ; sinon, elle garde la référence sélectionnée. Un MBID résolu de manière unique reste utilisable si cet artiste quitte ensuite les tags locaux. Une ancienne référence limitée au nom sans preuve d’identité actuelle n’est pas devinée depuis un homonyme.

Les anciennes références limitées à un MBID devenu ambigu sont conservées plutôt que réattribuées automatiquement : elles ne prouvent plus quel artiste local avait été sélectionné. Une annotation en attente reste supprimable par son identifiant de relation sauvegardé.

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
