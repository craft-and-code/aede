# analyze — Analyser avec FlacCompagnon

analyze lance le moteur Rust de [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/) sur les albums catalogués. Il mesure l’audio et conserve les résultats attribués pour les détails de pistes et les décisions de niveau à la lecture. Les résultats restent dans Aède même sans option de rapport.

Les pistes inchangées avec un rapport valable sont réutilisées. --force relance les mesures choisies. --json crée un rapport dans chaque dossier d’album ; --json-layout artist l’enregistre dans le dossier parent d’artiste. Un rapport existant non réutilisable/remplaçable peut nécessiter --force. L’écriture exige les droits à côté de la musique ; aucun tag audio n’est modifié.

--show-results affiche les mesures par piste. --threads contrôle les analyses parallèles. Les rapports d’artiste peuvent couvrir plusieurs albums ; scanner leur dossier ou les importer pour rattacher les résultats. Le résultat daté le plus récent gagne en cas de recouvrement. Supprimer un rapport n’efface pas les mesures déjà importées ; modifier l’audio les rend obsolètes. Certains résultats peuvent être enregistrés avant un échec : le travail terminé est gardé et l’erreur récapitulée.

Les mesures réutilisées gardent leur date d’origine, y compris dans les rapports JSON réécrits et lors des scans suivants. Chaque entrée écrite par Aède contient une chaîne décimale exacte `aede_result_at_ns` ; les rapports sans ce champ utilisent leur date de modification. L’affichage de progression et de résultats neutralise les caractères de contrôle du terminal sans modifier les mesures conservées ni les exports JSON. Voir le [contrat des rapports](../../imported-analyses.md#re-running-an-analysis-and-keeping-its-results).

La réutilisation conserve l’entrée source complète, y compris les mesures imbriquées inconnues. Un rapport explicitement limité à une sélection partielle `analyses_run` est réanalysé pour fournir toutes les mesures demandées par cette commande. Les anciens rapports sans cette métadonnée gardent la convention de couverture complète de FlacCompagnon ; l’import d’un rapport partiel reste pris en charge.

Si un fichier prévu pour réutilisation change ou disparaît pendant le décodage d’autres fichiers, la commande indique ce chemin et se termine en erreur. Les mesures terminées restent enregistrées dans Aède, mais le rapport de l’album concerné n’est ni créé ni remplacé par un sous-ensemble incomplet. Scanner les fichiers modifiés puis réessayer. Un fichier nouvellement décodé n’apparaît qu’une fois ; un ancien cache redevenu compatible pendant le décodage ne peut remplacer ni son résultat ni sa date de mesure.

Avant le décodage, les demandes d’écriture de rapports vérifient toutes les destinations résolubles et refusent les noms JSON réservés au catalogue, aux conclusions, aux sources, aux données personnelles, aux comptes et au contrôle parental dans le dossier de données de cette installation, même avec `--force`. Cela couvre les alias du dossier parent et les variantes de casse. Choisir une autre disposition JSON ou un autre dossier de données si les chemins se confondent. Les noms ordinaires de rapports dans ce même dossier restent autorisés. Un dossier d’album indisponible n’empêche pas l’analyse des autres albums : son échec est signalé et la commande termine en échec tout en conservant le travail terminé. Chaque destination est revérifiée avant publication, sans recréer un dossier d’album indisponible.

## Syntaxe et arguments

```text
aede analyze [folder…] [--json | --json-layout album|artist] [--force] [--show-results] [--threads N]
```

Zéro ou plusieurs dossiers d’album/artiste catalogués. Sans argument : albums du catalogue.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--json / -j` | Enregistrer des rapports JSON à côté des albums. Cela ne transforme pas stdout en flux uniquement JSON. |
| `--threads N` | Nombre de traitements parallèles. Un entier positif fixe ce nombre ; 0 le choisit automatiquement. La copie simple utilise un traitement par défaut. |
| `--force` | Réanalyser les albums sélectionnés malgré les résultats réutilisables ; permettre le remplacement des rapports sélectionnés. |
| `--show-results` | Afficher les mesures de chaque piste en plus de la progression. |
| `--json-layout album\|artist` | Enregistrer les rapports JSON dans chaque dossier d’album (album) ou le dossier parent d’artiste (artist). Active l’enregistrement même sans --json. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede analyze "$HOME/Music/Jazz"
aede analyze "$HOME/Music/Jazz" --show-results
aede analyze "$HOME/Music/Jazz" --json-layout album --threads 2
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[import](import.md), [track](track.md), [play](play.md).

Guide détaillé existant : [imported-analyses.md](../../imported-analyses.md).
