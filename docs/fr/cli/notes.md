# notes — Exporter et parcourir ses notes

notes liste les notes et peut filtrer les entités portant --tag ÉTIQUETTE. CSV/JSON exportent les lignes montrées, pagination incluse. --export écrit plutôt le document personnel portable complet ; --import le fusionne avec les données actuelles sans vider le magasin.

En conflit, la note la plus récente gagne et les remplacements sont signalés ; le plus grand compteur d’écoutes est conservé. Importer deux fois le même fichier n’ajoute rien. L’export dépasse le simple texte Markdown car il garde la structure d’annotation permettant le rattachement.

note TYPE NOM lit/écrit une note précise ; backup sauvegarde les quatre magasins ; rules les décisions reproductibles de sources/graphe. Garder les exports privés : notes et références peuvent être personnelles.

## Syntaxe et arguments

```text
aede notes
```

Aucun argument positionnel ; --import fournit un JSON personnel portable.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |
| `--export` | Écrire le document personnel portable complet, pas seulement les notes paginées. |
| `--import FILE` | Fusionner un export personnel JSON compatible : note récente et compteur maximal conservés ; import identique sans doublon. |
| `--tag LABEL[,LABEL]` | Avec notes/relations, filtrer une étiquette personnelle ; avec relation, ajouter les étiquettes séparées par des virgules, ou les retirer avec --remove. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede notes --tag jazz
aede notes --export --output personal.json
aede notes --import personal.json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[note](note.md), [backup](backup.md), [rules](rules.md).

Guide détaillé existant : [annotating.md](../../annotating.md).
