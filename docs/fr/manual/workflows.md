# Usages quotidiens

Remplacer titres/dossiers par ceux présents. Choisir un même dossier de données pour chaque commande coopérante ; ajouter `--data` partout ou définir `AEDE_HOME` dans l’environnement.

## Ajouter des fichiers

Ranger les fichiers selon votre arborescence, puis `aede scan` s’ils sont sous une racine suivie. Une nouvelle racine : `aede scan "/chemin/autre-bibliotheque"`. Ensuite `aede doctor` signale les problèmes sans retaguer ni supprimer les doublons. Aède ne réorganise pas les originaux depuis les tags.

## Relier les personnes d’un album

```sh
aede fetch --credits "/path/to/album"
aede credits "Album title"
aede album "Album title"
aede relations "Contributor name"
```

Les crédits d’enregistrement concernent interprétation/production, ceux d’œuvre la création, ceux de parution une édition exacte. Copier les identifiants dans Continue. review résout les identités incertaines ; credit corrige précisément en gardant la preuve. La comparaison de discographie exige `fetch --discography` avant `missing`.

## Emporter une sélection

```sh
aede collection Road --query "loved"
aede copy "/Volumes/Player" --collection Road --dry-run
aede copy "/Volumes/Player" --collection Road --verify
```

Lire la prévision avant une grosse copie. Pour alléger : `--compress mp3 --quality V0` ; FFmpeg requis, seules les sources sans perte sont réencodées. La destination reste hors des racines. Une collection est dynamique : de nouvelles pistes correspondantes rejoignent l’évaluation suivante.

## Réutiliser les mesures FlacCompagnon

```sh
aede import "/path/to/artist-report.json"
aede scan "/path/to/music"
aede import --list
aede track "Track title"
```

Ou `aede analyze "/path/to/album" --show-results` mesure directement. Enregistrer des rapports avec `--json-layout album|artist`. LUFS/true peak actuels peuvent informer la normalisation ; les mesures obsolètes sont exclues. L’analyse diffère du contrôle de somme : check vérifie ses CRC pris en charge ; lire les verdicts.

## Garder l’API de catalogue ouverte

```sh
aede scan "/path/to/music" --data "/path/to/aede-data"
aede serve --data "/path/to/aede-data"
```

Un second terminal sert aux requêtes et écritures CLI utilisant les mêmes données. Sur Unix, les écritures sont déléguées. Fermer cette CLI n’annule pas scan/fetch ; cancel utilise l’identifiant affiché. Le serveur reste local, sans route de lecture audio. Lire [serve](../cli/serve.md) avant essais de service/NAS.

## Avant une modification importante

Créer `aede backup before-change.aede` et sauvegarder l’audio séparément. `notes --export` transfère les annotations ; `rules --export` les décisions ; `export --graph` inspecte le graphe attribué. Lire leur portée avant de les traiter comme sauvegarde de récupération.
