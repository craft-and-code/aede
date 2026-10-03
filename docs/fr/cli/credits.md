# credits — Auditer les crédits

credits audite hors ligne la couverture des crédits MusicBrainz d’enregistrement/œuvre et d’édition exacte. Sans sélection, les comptes sont regroupés par album ; avec titre, identifiant d’édition ou dossier, il détaille enregistrements et fichiers représentatifs. Les comptes d’enregistrement portent sur les enregistrements canoniques, pas chaque fichier dupliqué. Les comptes d’édition portent sur les sorties locales : deux éditions partageant les mêmes enregistrements restent distinctes.

credited signifie crédits utilisables ; empty, recherche terminée sans crédit ; waiting, recherche à faire ; untrusted, preuve présente non admissible ; unidentified, identité d’enregistrement insuffisante. Ces états distinguent absence de crédits et absence de recherche. Crédits d’enregistrement et d’œuvre sont séparés.

La couverture d’édition utilise les mêmes états, mais exige un identifiant MusicBrainz de sortie locale non vide et une réponse terminée, approuvée, pour cette édition exacte. Une identité de groupe de sorties ou une interrogation d’enregistrement ne termine pas celle de l’édition. Les crédits manuels d’édition sont comptés séparément : ils ne prétendent pas que MusicBrainz a été interrogé.

Pour un enregistrement ou une édition en attente, copier la commande fetch ciblée affichée. --full appartient à fetch si vous voulez répéter une réponse terminée, pas à credits. --json conserve les champs d’enregistrement et ajoute la couverture d’édition pour les outils ; aucune demande réseau.

## Syntaxe et arguments

```text
aede credits [album title|MusicBrainz ID|folder] [--json]
```

Titre d’album, identifiant MusicBrainz de parution ou dossier existant facultatifs.

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
aede credits
aede credits "Kind of Blue"
aede credits "$HOME/Music/Jazz" --json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[fetch](fetch.md), [credit](credit.md), [recording](recording.md), [work](work.md).

Guide détaillé existant : [commands.md](../../commands.md).
