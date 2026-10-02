# rules — Partager ses décisions

rules liste les choix humains reproductibles, les exporte dans un fichier versionné ou fusionne ce fichier. Il inclut décisions de sources, sources manuelles, exclusions de crédits, règles de classement d’artistes, parutions manquantes mises de côté, requêtes enregistrées et annotations de relations.

C’est différent de backup : historique, notes/favoris/étoiles ordinaires des entités et biographies/images externes sont exclus. Exporter les notes séparément ou utiliser backup pour une photographie complète.

--export écrit dans le terminal ou --output FICHIER. --import FICHIER fusionne un document aede-rules compatible et compte éléments ajoutés, actualisés, conservés et décisions importées. Export/import sont incompatibles ; --output sans export est refusé. Garder une sauvegarde avant d’importer des décisions à annuler ensuite individuellement.

## Syntaxe et arguments

```text
aede rules
```

Aucun argument positionnel ; --import fournit son fichier.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--export` | Exporter le document mémorisé propre à cette commande. Ce n’est pas le même résultat qu’une liste affichée en CSV/JSON. |
| `--import FILE` | Fusionner un document exporté depuis FICHIER ; consulter les règles de conflit précisées ci-dessous. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede rules
aede rules --export --output rules.json
aede rules --import rules.json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[backup](backup.md), [notes](notes.md), [review](review.md).

Guide détaillé existant : [commands.md](../../commands.md).
