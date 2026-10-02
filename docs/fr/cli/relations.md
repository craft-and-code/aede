# relations — Lister le graphe relationnel

relations inventorie les liens dirigés du graphe : relations des tags et déclarations externes/manuelles. Chaque ligne garde identifiant stable, type, extrémités, provenance et confiance ; une simple liste de personnes perdrait ce contexte.

Le nom facultatif cherche dans les deux extrémités et le type. --source limite la provenance, par exemple musicbrainz ; --tag limite vos étiquettes de liens. --json fournit les liens structurés et --output exige cette forme JSON. La pagination s’applique à la liste obtenue.

Ouvrir un identifiant avec relation, puis l’annoter sans modifier le lien. Pour corriger un crédit de source, employer credit --exclude plutôt que relation --remove : ce dernier retire seulement votre annotation. Un lien disparu ne supprime pas son annotation ; doctor signale les orphelines, encore retirables par identifiant.

## Syntaxe et arguments

```text
aede relations [name]
```

Texte de nom facultatif ; un identifiant précis appartient à relation.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--output FILE / -o FILE` | Écrire les liens structurés avec --json ; refusé pour la page humaine normale. |
| `--source NAME` | Filtrer selon le nom de source/d’outil mémorisé. Employer les noms affichés dans la liste correspondante. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--tag LABEL[,LABEL]` | Avec notes/relations, filtrer une étiquette personnelle ; avec relation, ajouter les étiquettes séparées par des virgules, ou les retirer avec --remove. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede relations "Miles Davis"
aede relations --source musicbrainz --all
aede relations --tag dubious --json --output relations.json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[relation](relation.md), [credit](credit.md), [doctor](doctor.md).

Guide détaillé existant : [commands.md](../../commands.md).
