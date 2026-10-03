# backup — Sauvegarder les données

backup crée un fichier versionné contenant catalogue, conclusions, données personnelles, preuves de sources et comptes lisibles. Il fonctionne même avec seulement certains magasins ; notes ou comptes sans catalogue valent aussi une sauvegarde. Le résumé distingue contenu présent, absent ou illisible.

La version 3 inclut les vérificateurs salés des mots de passe, jamais les sessions. Une sauvegarde avec identifiants exige des permissions Unix privées, y compris pour remplacer une destination existante (`chmod 600`). Les anciennes versions restent lisibles. Voir [accounts](accounts.md).

Choisir un nom explicite et conserver une copie hors de l’ordinateur/NAS. Si ce fichier existe, Aède demande avant de l’écraser ; --yes évite cette question. Un dossier entièrement vide produit une erreur plutôt qu’une sauvegarde trompeuse.

Le dossier parent de la destination doit déjà exister. Les magasins et verrous actifs d’Aède, l’audio existant, les liens symboliques finaux et les fichiers spéciaux sont refusés même avec `--yes`. L’archive complète passe par un fichier temporaire isolé avant remplacement atomique ; un autre nom lié au même fichier conserve ses octets d’origine. Sous Unix, une nouvelle sauvegarde ne donne aucun accès aux autres utilisateurs ; les permissions d’une destination existante sont conservées. Cela ne garantit pas la durabilité après une coupure de courant.

Le fichier ne contient ni audio original, ni images, paroles annexes ou autres fichiers dérivés. Les sauvegarder séparément. Garder ce fichier privé : il peut inclure historique personnel et chemins absolus. Employer restore pour récupérer les données ; l’export simple du catalogue ne contient pas toutes les informations personnelles irremplaçables.

## Syntaxe et arguments

```text
aede backup <file>
```

Exactement un fichier de destination en argument, pas --output. Les arguments supplémentaires sont refusés.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--yes` | Accepter la confirmation demandée par cette commande. Vérifier l’opération auparavant ; réserver aux exécutions automatisées maîtrisées. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede backup aede-backup.aede
aede backup "/Volumes/Backup/aede-2026-10.aede"
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[restore](restore.md), [export](export.md), [rules](rules.md).

Guide détaillé existant : [operating.md](../../operating.md).
