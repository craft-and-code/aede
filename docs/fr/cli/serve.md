# serve — Démarrer le serveur local

serve expose le catalogue existant sur la même machine à http://127.0.0.1:8787. Scanner au moins un dossier auparavant : le démarrage refuse sans catalog.json. Serveur et CLI doivent utiliser le même dossier de données. --port 0 choisit un port libre et l’affiche.

Le serveur actuel propose des routes de catalogue JSON et des notifications WebSocket de catalogue/activité. Aucune route de lecture/diffusion audio, aucun compte auditeur ni accès distant pris en charge. Les autres utilisateurs locaux peuvent lire métadonnées et chemins. Un NAS/Raspberry Pi demande encore validation sur cible ; aucune image conteneur Aède publiée n’est fournie.

Sur Unix, les CLI qui écrivent dans les données du même compte délèguent au serveur via sa socket privée. Fermer cette CLI n’arrête pas le travail accepté ; scan/fetch affichent un identifiant utilisable par cancel. Ctrl-C ou SIGTERM sur le serveur bloque le nouveau travail et attend les tâches acceptées. AEDE_ADMIN_TOKEN, secret ASCII d’au moins 32 caractères défini avant démarrage, active une administration authentifiée distincte ; la CLI locale normale n’en a pas besoin. Lire le guide serveur avant de configurer écritures HTTP ou services.

## Syntaxe et arguments

```text
aede serve [--port N]
```

Aucun argument positionnel. Le démarrer dans un terminal gardé ouvert ou un service adapté à votre système.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--port N` | Port HTTP local, entier de 0 à 65535. Par défaut 8787. Avec 0, le système choisit un port libre ; lire l’adresse affichée. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede scan "$HOME/Music"
aede serve
aede serve --port 0
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[scan](scan.md), [cancel](cancel.md), [backup](backup.md).

Guide détaillé existant : [operating.md](../../operating.md).
