# serve — Démarrer le serveur de catalogue et audio authentifié

serve expose le catalogue existant sur la même machine à http://127.0.0.1:8787. Scanner au moins un dossier auparavant : le démarrage refuse sans catalog.json. Serveur et CLI doivent utiliser le même dossier de données. --port 0 choisit un port libre et l’affiche.

Le serveur propose un catalogue JSON, des notifications WebSocket et un [contrat audio authentifié](../server/playback.md). Les [comptes](accounts.md) protègent l’accès au catalogue et isolent les données personnelles ; sans comptes, les autres utilisateurs locaux peuvent lire métadonnées et chemins. Une [configuration HTTPS explicite](../server/remote.md) active l’accès distant avec comptes obligatoires. Un NAS/Raspberry Pi demande encore validation sur cible ; aucune image conteneur Aède publiée n’est fournie.

Sur Unix, les CLI qui écrivent dans les données du même compte délèguent au serveur via sa socket privée. Fermer cette CLI n’arrête pas le travail accepté ; scan/fetch affichent un identifiant utilisable par cancel. Ctrl-C ou SIGTERM sur le serveur bloque le nouveau travail et attend les tâches acceptées. AEDE_ADMIN_TOKEN, secret ASCII d’au moins 32 caractères défini avant démarrage, active une administration authentifiée distincte ; la CLI locale normale n’en a pas besoin. Lire le guide serveur avant de configurer écritures HTTP ou services.

Un compte administrateur autorise aussi les tâches de l’installation en HTTP local. HTTPS désactive toute la famille `/api/admin` et le jeton historique ; utilisez la CLI locale de confiance pour l’administration. Un `user` peut modifier seulement ses données personnelles ; un `auditor` peut lire seulement le catalogue et ses vues `/api/me/v1` en `GET`/`HEAD`. Les [sessions](../server/accounts.md) utilisent un en-tête Bearer pour le catalogue et `/api/me/v1` pour les données personnelles. La CLI de comptes prend directement le verrou partagé, sans délégation.

## Syntaxe et arguments

```text
aede serve [--bind IP] [--port N] [--tls-cert PATH --tls-key PATH --authority HOST:PORT]
```

Aucun argument positionnel. Le démarrer dans un terminal gardé ouvert ou un service adapté à votre système.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--port N` | Port d’écoute, entier de 0 à 65535 ; défaut 8787. HTTP accepte 0 pour un port libre. TLS refuse 0. |
| `--bind IP` | Adresse IP littérale d’écoute ; défaut `127.0.0.1`. Hors boucle locale, TLS obligatoire. |
| `--tls-cert PATH` | Chaîne de certificats PEM. Exige `--tls-key` et `--authority`. |
| `--tls-key PATH` | Clé privée PEM correspondante, protégée et non chiffrée. |
| `--authority HOST:PORT` | Adresse HTTPS publique exacte, sans schéma ni chemin ; IPv6 entre crochets. Elle peut différer de l’adresse/du port d’écoute. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede scan "$HOME/Music"
aede serve
aede serve --port 0
aede serve --bind 0.0.0.0 --port 8787 --tls-cert /private/aede/fullchain.pem --tls-key /private/aede/key.pem --authority music.example:8787
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[scan](scan.md), [cancel](cancel.md), [backup](backup.md), [configuration HTTPS](../server/remote.md), [contrat audio](../server/playback.md).

Guide détaillé existant : [operating.md](../../operating.md).
