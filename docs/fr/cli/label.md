# label — Explorer un label

label ouvre les parutions/artistes locaux et précise si l’identité vient d’un tag, d’une recherche exacte, d’une proposition de nom ou d’un conflit. Propositions/conflits ne deviennent pas silencieusement confiance ; review décide.

À la différence des pages locales ordinaires, label peut vérifier un profil Discogs nécessaire à l’ouverture. --offline interdit le réseau ; --online demande une vérification fraîche et est incompatible avec --offline. Le texte garde son attribution et liens de labels/artistes ; les tags ne changent pas.

CSV/JSON/M3U exportent les pistes locales liées, pas tout le profil externe. Utiliser l’identité précise devant les homonymes. fetch --labels récupère les identités MusicBrainz ; le profil Discogs est un recours contextuel sans texte Wikipedia.

## Syntaxe et arguments

```text
aede label <name> [--offline]
```

Un nom de label.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--online` | Vérifier maintenant le profil Discogs du label plutôt que se limiter au résultat mémorisé. |
| `--offline` | Ouvrir le label sans vérification réseau. Incompatible avec --online. |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--m3u` | Produire une playlist M3U des pistes locales sélectionnées. Elle contient des chemins, pas une copie de la musique. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede label "Blue Note" --offline
aede label "Blue Note" --online
aede fetch --labels
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[labels](labels.md), [review](review.md), [fetch](fetch.md).

Guide détaillé existant : [browsing.md](../../browsing.md).
