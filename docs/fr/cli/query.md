# query — Faire une recherche relationnelle

query sélectionne des pistes avec le langage d’expression Aède. L’espace combine par ET ; OR et les parenthèses expriment les alternatives ; - exclut une condition. Citer l’expression entière pour que le terminal n’interprète ni parenthèses ni comparaisons.

Champs texte : title, artist, album, genre, label, comment, lyrics, path. Les nombres acceptent comparaisons/intervalles, par exemple year:1990..1999, duration:..4:00 et rating:>=4. Le graphe propose recording, work, releasegroup, instrument, producer, composer, performing, guest, contributor. Une entité précise inexistante peut provoquer une erreur plutôt qu’un faux résultat vide.

rating/tag/note seuls ciblent les pistes ; album.rating ou artist.note changent le niveau. loved hérite des favoris piste, album ou artiste ; track.loved le limite. Les paroles viennent des tags/.lrc. Les liens de sources de confiance participent ; propositions en attente/refusées restent seulement preuves.

Enregistrer les questions récurrentes avec collection. --m3u exporte les chemins ; --csv/--json les lignes sélectionnées ; la pagination affecte l’export. Tris : title, artist, album, year, duration, size, rating, played, catalog, suffixe - pour inverser. Le guide lié fait référence pour tous les champs/opérateurs.

Alias : `aede find`. Options et comportement identiques.

## Syntaxe et arguments

```text
aede query <expression>
```

Une expression complète. Préférer des guillemets autour ; échapper les guillemets imbriqués.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--m3u` | Produire une playlist M3U des pistes locales sélectionnées. Elle contient des chemins, pas une copie de la musique. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |
| `--sort ORDER` | Choisir une colonne disponible ; ajouter - pour décroître, par exemple duration-. Les valeurs de cette commande figurent ci-dessous. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede query "genre:jazz year:1950..1960"
aede query "loved played:0" --m3u --output unheard.m3u8
aede query "album.rating:>=4" --sort duration-
aede query "producer:Rick" --json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[collection](collection.md), [search](search.md), [copy](copy.md).

Guide détaillé existant : [querying.md](../../querying.md).
