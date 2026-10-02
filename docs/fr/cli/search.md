# search — Chercher simplement

search retrouve artistes, albums, pistes, enregistrements, œuvres et groupes de parution par texte libre. Les contributeurs/œuvres seulement sourcés restent distincts des identités locales. Chaque résultat propose Open ; suivre l’identifiant précis devant les homonymes.

Commentaires, notes et paroles sont optionnels via --comments, --notes, --lyrics. Les paroles montrent les lignes trouvées plutôt que toute la chanson. --json garde l’origine found_in des textes. Il lit les informations mémorisées/annexes sans télécharger biographies ni paroles manquantes.

CSV/M3U exportent seulement les pistes trouvées, pas tous les résultats d’artistes/albums ; JSON peut garder les catégories. La pagination affecte les résultats. query convient aux intervalles, OU, exclusions et rôles relationnels.

## Syntaxe et arguments

```text
aede search <text>
```

Mots à chercher ; citer le texte composé.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--m3u` | Produire une playlist M3U des pistes locales sélectionnées. Elle contient des chemins, pas une copie de la musique. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--comments` | Chercher aussi dans les commentaires des fichiers ; désactivé par défaut. |
| `--notes` | Chercher aussi dans vos notes personnelles ; désactivé par défaut. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |
| `--lyrics` | Avec track, afficher les paroles ; search, chercher leur texte ; fetch, télécharger les fichiers .lrc manquants depuis LRCLIB. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede search coltrane
aede search remaster --notes --comments
aede search train --lyrics
aede search coltrane --json --output search.json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[query](query.md), [artist](artist.md), [track](track.md).

Guide détaillé existant : [querying.md](../../querying.md).
