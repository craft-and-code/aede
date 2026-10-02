# stats — Comprendre les statistiques

stats décrit tout le catalogue : pistes, albums, artistes, durée, espace et catégories de qualité. Les catégories sans perte, haute résolution et avec perte décrivent les fichiers ; elles ne prouvent ni qualité audible ni origine de l’enregistrement.

Après le premier scan, vérifier que les volumes correspondent aux dossiers attendus. La pagination limite les listes secondaires, sans choisir une autre bibliothèque. --json fournit un rapport structuré ; stats ne possède pas d’option d’export --output. La redirection du terminal peut enregistrer sa sortie.

Aucune métadonnée ni musique n’est modifiée. Si un nombre surprend, consulter roots et doctor, puis refaire un scan. Un catalogue absent demande un scan ; --data ne désigne pas le dossier musical à lire.

## Syntaxe et arguments

```text
aede stats
```

Aucun argument positionnel ; les statistiques concernent le dossier de données choisi.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--limit N` | Afficher au plus N lignes ; entier strictement positif. La limite par défaut dépend de la page, généralement 50. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le résultat structuré JSON. Avec analyze, enregistrer des rapports plutôt que changer l’affichage du terminal. |

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
