# stats — Comprendre les statistiques

stats décrit tout le catalogue : pistes, albums, artistes, durée, espace et catégories de qualité. Les catégories sans perte, haute résolution et avec perte décrivent les fichiers ; elles ne prouvent ni qualité audible ni origine de l’enregistrement.

Après le premier scan, vérifier que les volumes correspondent aux dossiers attendus. La pagination s’applique séparément à chaque liste secondaire (formats, qualité, fréquences, décennies, pays, rôles et classements), en affichage humain comme en JSON. Les totaux et proportions de la bibliothèque restent complets. --json fournit un rapport structuré ; stats ne possède pas d’option d’export --output. La redirection du terminal peut enregistrer sa sortie.

Le poids du dossier de données inclut `catalog.json`, `conclusions.json`, `user.json` et `sources.json`. Les durées et tailles cumulées plafonnent au plus grand entier pris en charge si des mesures malformées ou importées dépassent cette capacité ; les valeurs usuelles sont additionnées exactement.

Aucune métadonnée ni musique n’est modifiée. Si un nombre surprend, consulter roots et doctor, puis refaire un scan. Un catalogue absent demande un scan ; --data ne désigne pas le dossier musical à lire.

## Syntaxe et arguments

```text
aede stats
```

Aucun argument positionnel ; les statistiques concernent le dossier de données choisi.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--limit N` | Afficher au plus N lignes par liste secondaire ; entier strictement positif. Valeur par défaut : 10. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes à partir de l’offset demandé. Incompatible avec --limit. |
| `--json / -j` | Produire les statistiques structurées avec la même pagination des listes. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede stats
aede stats --json
aede stats --json > stats.json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[scan](scan.md), [doctor](doctor.md).

Guide détaillé existant : [library.md](../../library.md).
