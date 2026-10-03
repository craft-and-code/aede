# scan — Indexer la musique

scan parcourt récursivement les fichiers audio pris en charge, construit le graphe depuis leurs tags et enregistre le catalogue dans le dossier de données. Le premier scan exige au moins un dossier musical lisible. Sans argument, les scans suivants reprennent les dossiers suivis. Un dossier ajouté ensuite complète cette liste.

Le scan normal réutilise les fichiers inchangés. --full relit les métadonnées tout en gardant les dossiers suivis. --replace conserve seulement ceux nommés dans cet appel : donner tous ceux à garder. Les exclusions restent appliquées. Un disque suivi débranché est signalé et ses anciennes entrées sont conservées pendant le scan des autres racines. Un nouveau dossier indisponible est refusé ; retirer volontairement une racine retire ses entrées.

Le résumé indique fichiers, albums, artistes et problèmes de lecture. Un scan réussi produit un index, pas un verdict d’intégrité audio. Les rapports FlacCompagnon présents sont rattachés aux fichiers correspondants ; les fichiers modifiés rendent leurs anciennes conclusions obsolètes. L’audio et ses tags restent intacts.

## Syntaxe et arguments

```text
aede scan [folder…]
```

Zéro ou plusieurs chemins de dossiers musicaux existants. Entourer de guillemets les chemins contenant des espaces.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--dry-run` | Afficher ajouts, changements, suppressions et chemins illisibles, sans enregistrer catalogue, racines ou données personnelles. Les métadonnées peuvent être lues pour préparer le résultat. |
| `--json / -j` | Produire le même bilan structuré, lors d’une prévision ou d’un scan réel. |
| `--full` | Relire les tags au lieu de réutiliser les fichiers inchangés ; garder racines et magasins séparés. |
| `--threads N` | Nombre de traitements parallèles. Un entier positif fixe ce nombre ; 0 le choisit automatiquement. La copie simple utilise un traitement par défaut. |
| `--replace` | Garder seulement les dossiers explicitement fournis à ce scan ; les exclusions existantes restent. |
| `--follow-symlinks` | Lors du scan, suivre les liens symboliques vers des fichiers ou dossiers. Sans cette option, ils ne sont pas suivis. |
| `--include-hidden` | Inclure les fichiers et dossiers cachés lors du scan ; ils sont ignorés par défaut. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede scan "$HOME/Music"
aede scan "/Volumes/Archive musicale"
aede scan
aede scan --full
aede scan --dry-run --json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[roots](roots.md), [check](check.md), [analyze](analyze.md).

Guide détaillé existant : [library.md](../../library.md).

Un sous-dossier inaccessible conserve ses entrées déjà connues, même avec --full, et une erreur de lecture est signalée. Les horodatages précis sont utilisés lorsque le système les fournit ; un ancien catalogue sans cette précision est relu une fois. Le JSON contient dry_run, roots, added, changed, removed, unreadable et counts ; preserved compte les entrées conservées des chemins inaccessibles.

Seuls les fichiers réguliers sont ouverts : un fichier spécial portant une extension audio ne peut pas bloquer le scan. Les chemins non représentables en UTF-8 sont signalés et ignorés ; les racines non UTF-8 sont refusées. Avec --follow-symlinks, les alias sont choisis dans un ordre natif déterministe. Un fichier changé ou remplacé pendant la lecture est signalé comme temporairement illisible et son entrée connue est conservée.
