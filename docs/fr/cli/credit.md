# credit — Corriger un crédit

credit enregistre un crédit manuel attribué ou exclut/restaure un crédit de source précis. Consulter relations pour copier l’identifiant stable et recording/work/album pour trouver l’identité exacte de la cible.

Choisir une seule action. --add prend une portée en argument recording:ID, work:ID ou release:ID et exige --artist/--role. L’œuvre doit être liée à un enregistrement local ; l’édition doit être présente. --artist-id et --instrument précisent la déclaration. Un crédit manuel identique ne peut pas être ajouté deux fois. Le contrôle des doublons suit la portée musicale actuellement attachée à l’enregistrement ou à l’œuvre, même si l’ajout d’un fichier change son représentant. Un crédit exclu reste présent et se restaure avec `--undo` ; les éditions conservent leur portée locale exacte.

Ajouter ensuite un MBID d’enregistrement ne permet pas de dupliquer un crédit CLI initialement ciblé par un fichier local encore présent. Son identifiant de relation et son exclusion restent intacts. Ce contrôle suit uniquement les alias des placements actuels ; il ne devine pas d’anciens MBID et ne fusionne pas les identifiants arbitraires de relations manuelles importées.

Un crédit `work:ID` concerne la composition et s’applique aux enregistrements locaux liés à ce MBID d’œuvre exact, par leurs tags ou des preuves fiables. Il reste une seule déclaration attribuée, avec son hôte d’origine et une exclusion réversible ; des titres identiques sous des identifiants différents n’étendent pas sa portée. Un crédit `recording:ID` ou `release:ID` reste limité à cette interprétation ou édition.

Si un instantané manuel d’édition conservé porte déjà un MBID d’édition, modifier ou retirer ce tag ne déplace pas ses anciens crédits. Ajouter un autre crédit au même album local est refusé tant que l’édition courante diffère, afin de ne pas réattribuer les déclarations conservées. Les instantanés manuels créés sans identifiant d’édition gardent leur comportement d’attachement local existant.

Les nouveaux crédits directs d’enregistrement conservent son MBID lorsqu’il est connu. Changer ou retirer cet identifiant ne transfère pas ces crédits à la nouvelle interprétation du fichier : les preuves restent enregistrées et un autre crédit direct est refusé tant que l’identité capturée diffère. Les crédits d’œuvre sur la même observation gardent leur portée indépendante. Les anciens instantanés ou crédits locaux sans identifiant capturé restent attachés au chemin ; une lecture n’infère ni ne migre une identité antérieure.

--exclude ID retire cette assertion de navigation/recherche tout en conservant sa preuve ; les crédits lus dans les tags ne peuvent pas être exclus. --undo ID restaure une exclusion. Ces actions refusent les champs de nouveau crédit et la portée positionnelle. Pour corriger : exclure la source précise, puis ajouter la correction attribuée. Les décisions passent par rules ; les tags restent intacts.

## Syntaxe et arguments

```text
aede credit --add recording:<ID>|work:<ID>|release:<ID> --artist=<name> --role=<role> [--artist-id=<MBID>] [--instrument=<name>] | aede credit --exclude=<credit ID> | --undo=<credit ID>
```

Avec --add : exactement une portée. Remplacer RECORDING_ID/CREDIT_ID par les identifiants affichés ; aucune chevron dans la vraie saisie.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--undo ID` | Annuler cette décision (review) ou restaurer ce crédit de source exclu (credit). |
| `--add` | Ajouter un crédit manuel. Donner exactement une portée en argument, telle que recording:<ID> ; artiste et rôle requis. |
| `--artist-id MBID` | Identifiant MusicBrainz facultatif pour le crédit manuel ; le récupérer dans la navigation artiste ou MusicBrainz. |
| `--instrument NAME` | Instrument/attribut facultatif associé au crédit manuel. |
| `--role ROLE` | Garder les personnes ou contributions portant ce rôle ; avec credit, définir le nouveau rôle. |
| `--artist NAME` | Filtrer par nom d’artiste avec albums/track ; avec credit, nommer la personne créditée. Les noms composés sont acceptés. |
| `--exclude FOLDER_OR_ID` | Exclure le dossier indiqué (roots) ou le crédit provenant d’une source (credit), sans modifier l’audio. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede relations --source musicbrainz
aede credit --add recording:RECORDING_ID --artist "Jane Doe" --role producer
aede credit --exclude CREDIT_ID
aede credit --undo CREDIT_ID
```

Les valeurs ID en majuscules sont des exemples. Copier le vrai identifiant affiché ; ne pas saisir l’exemple ni des chevrons.

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[relations](relations.md), [recording](recording.md), [work](work.md), [rules](rules.md).

Guide détaillé existant : [commands.md](../../commands.md).
