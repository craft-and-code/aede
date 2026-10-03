# Statistiques du projet

Ces mesures décrivent les sources du projet utilisées pour construire cette documentation. Elles sont générées à chaque publication, plutôt que maintenues à la main dans un README. Les comptes indiquent la taille du projet et le périmètre couvert ; ils ne mesurent pas la qualité du code et ne prouvent pas que les tests passent.

## Mesures générées

<!-- project-statistics -->

## Ce qui est compté

Les lignes de code sont des **lignes physiques**, commentaires et lignes vides compris, dans les fichiers source pris en charge sous `crates/`, `tools/` et `site/`. Les lignes non vides sont indiquées séparément. Le tableau par crate distingue production, tests/support et exemples. Les lignes de tests comprennent les tests d’intégration et les fixtures écrites dans un langage source : cette mesure est différente de l’inventaire des tests unitaires actifs.

Les fichiers générés, caches de compilation, fichiers tiers copiés, dossiers cachés, liens symboliques, musique/images, fixtures JSON, manifests/verrous Cargo et Markdown sont exclus des lignes de code. La documentation possède ses propres comptes de fichiers/pages. Une page publiée compte une fois par langue, y compris un repli anglais explicitement signalé quand un sujet n’est pas traduit en français. Le script réutilise le catalogue du site, sans maintenir une seconde liste.

Les **TU Rust actifs** proviennent des exécutables compilés de tests des bibliothèques et binaires du workspace. Le script liste les tests enregistrés, soustrait les tests ignorés et publie un seuil arrondi conservateur, par exemple « + de 1 400 TU actifs ». Il ne compte pas les occurrences de `#[test]`. Tests d’intégration, tests d’exemples, doctests et tests des outils Python restent hors de ce compte de TU, même s’ils participent à la vérification.

L’inventaire indique la chaîne Rust, la machine cible et les fonctionnalités de compilation : les tests conditionnels peuvent différer selon le système ou la configuration. Lister un test ne l’exécute pas. Le chiffre publié de TU n’est disponible qu’avec un inventaire des sources Rust/Cargo actuelles ; un inventaire absent ou périmé ne devient pas silencieusement un compte actuel. Voir le [processus de vérification](../../coding/engineering-rules.md) et l’[état courant](../../coding/current-state.md) pour les résultats de validation réellement obtenus.

## Reproduire les mesures

Depuis la racine du dépôt, Python 3.9 ou ultérieur suffit pour mesurer les sources, sans installation de paquet ni requête réseau :

```sh
python3 tools/project-stats.py
python3 tools/project-stats.py --json
```

Pour inclure les TU actifs, préparez les prérequis Rust/natifs existants décrits dans le [guide d’installation](install.md). Récupérez explicitement les dépendances verrouillées si elles ne sont pas déjà en cache, puis compilez/listez les exécutables de tests hors ligne :

```sh
cargo fetch --locked
python3 tools/project-stats.py --tests --output target/project-stats.json
python3 tools/build-site.py --check --project-stats target/project-stats.json
```

`--tests` n’exécute pas les corps des tests, ne joue aucun son et n’accède pas aux fichiers musicaux. Utilisez `tools/check.sh` pour les vérifications complètes ; il génère aussi l’inventaire et publie la documentation à partir de celui-ci. Le workflow Site génère son propre inventaire pour la compilation Linux avec fonctionnalités par défaut. Le JSON exploitable par les outils conserve les mesures exactes et leur provenance pour les contrôler ; l’affichage public des TU reste arrondi. Ne committez pas les rapports générés de `target/` ou `dist-site/`.

Pour une autre configuration, utilisez `--no-default-features` et/ou `--features "..."`. Cargo respecte `CARGO_TARGET_DIR` s’il est défini ; un autre inventaire se réutilise avec `--test-inventory PATH` uniquement tant que son empreinte des sources Rust/Cargo correspond. Une publication sans `--project-stats` génère toujours les statistiques fraîches des sources et laisse explicitement le nombre de TU indisponible.
