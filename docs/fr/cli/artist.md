# artist — Explorer un artiste

artist ouvre un artiste local par nom ou identifiant MusicBrainz. La carte distingue discographie, apparitions invitées, compilations, écriture/production et collaborations, plutôt que tout mélanger dans une liste d’albums.

--role limite les contributions, --with sélectionne les pistes communes à un autre artiste local, --members montre les membres datés. Les albums peuvent déduire la formation de leur année à partir des dates sourcées ; aucune formation historique n’est inventée.

Un contributeur présent seulement dans des crédits de confiance possède une carte de source. Utiliser son MBID si un homonyme local existe. --members/--with exigent des artistes locaux et sont refusés sur cette carte externe. CSV/M3U sélectionnent les pistes pertinentes, pas la biographie. La provenance reste visible ; Continue propose les objets voisins.

## Syntaxe et arguments

```text
aede artist <name|MusicBrainz artist ID>
```

Un nom d’artiste ou identifiant MusicBrainz. Remplacer l’identifiant d’exemple.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--csv` | Produire un tableau CSV de ce résultat pour un tableur ; choisir un seul format de sortie. |
| `--m3u` | Produire une playlist M3U des pistes locales sélectionnées. Elle contient des chemins, pas une copie de la musique. |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--role ROLE` | Garder les personnes ou contributions portant ce rôle ; avec credit, définir le nouveau rôle. |
| `--members` | Afficher les membres datés d’un groupe selon les sources de confiance. Nécessite un artiste local. |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |
| `--separator ";" / --separator tab` | Uniquement avec --csv : virgule par défaut, un caractère comme ;, ou tab pour des tabulations. Entourer ; de guillemets dans le terminal. |
| `--with NAME` | Afficher les pistes partagées avec un autre artiste local. Les deux artistes doivent être locaux. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede artist "Miles Davis"
aede artist "Miles Davis" --with "John Coltrane"
aede artist "A band" --members
aede artist MUSICBRAINZ_ARTIST_ID
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[artists](artists.md), [album](album.md), [relations](relations.md).

Guide détaillé existant : [browsing.md](../../browsing.md).
