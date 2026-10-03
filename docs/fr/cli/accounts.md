# accounts — Gérer les comptes, sessions et clés clients

Les comptes partagent le catalogue musical et conservent favoris, notes, étoiles, historique et collections sous leur propriétaire stable. Le premier administrateur récupère les données du propriétaire existant `local`. Renommer ou désactiver un identifiant conserve ses données personnelles. La CLI reste une interface de confiance du propriétaire système ; ses commandes personnelles existantes utilisent toujours `local`.

## Syntaxe et arguments

```text
aede accounts [list]
aede accounts init <name> --password-stdin
aede accounts create <name> <admin|user|auditor> --password-stdin
aede accounts password <name> --password-stdin
aede accounts role <name> <admin|user|auditor>
aede accounts rename <name> <new-name>
aede accounts enable <name>
aede accounts disable <name>
aede accounts revoke <name>
aede accounts keys <name>
aede accounts keys <name> create <label>
aede accounts keys <name> revoke <key-id>
```

`init` crée une seule fois le premier administrateur. `create` exige un rôle explicite. Un `admin` gère comptes et tâches de l’installation ; un `user` peut modifier uniquement ses données personnelles ; un `auditor` peut seulement lire le catalogue partagé et ses vues personnelles d’API. Un auditeur ne peut pas modifier son mot de passe par l’API, ses données personnelles, les comptes ni les tâches. Au moins un administrateur doit rester actif. `password` réinitialise un mot de passe ; `revoke` invalide sessions et clés API sans le modifier. Changer le nom, le rôle, le mot de passe ou l’état actif révoque les sessions et clés concernées. Il n’existe ni suppression de compte ni inscription publique ; désactiver conserve la propriété des données pour un retour ultérieur.

`keys` liste les clés OpenSubsonic persistantes d’un compte nommé. `keys … create <label>` sauvegarde une clé et affiche son secret complet une seule fois ; `keys … revoke <key-id>` retire uniquement cette clé. Le libellé est non blanc et limité à 128 octets UTF-8. Maximum : huit clés par compte, 512 au total. `--json` produit des tableaux de métadonnées pour liste/révocation, ou un objet de métadonnées avec `token` pour création. Gardez cette sortie de création privée. La liste ne demande pas de verrou ; création/révocation le prennent. Ces clés survivent au redémarrage, sans expiration automatique, et sont retirées par modification du compte, révocation globale du compte ou restauration. Voir [configuration client et méthodes disponibles](../server/subsonic.md).

Les identifiants acceptent 1–64 lettres ASCII, chiffres, points, tirets ou tirets bas, sans distinction de casse. Une lettre ou un chiffre est obligatoire. Un mot de passe contient au moins 15 caractères Unicode et au plus 1024 octets UTF-8, sans NUL. L’entrée accepte une ligne et retire un LF ou CRLF final ; les sauts de ligne internes sont refusés.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--password-stdin` | Lire le mot de passe depuis une entrée redirigée pour `init`, `create` ou `password`. La saisie directe depuis un terminal est refusée. Aucun mot de passe en argument. |
| `--json` | Les opérations de comptes donnent des tableaux de métadonnées sans mot de passe/vérificateur/jeton de session. Les clés utilisent les formes ci-dessus ; la création inclut le secret affiché une fois. |

La [référence des options](options.md) explique `--data`, couleurs et aide. La liste ne demande ni catalogue ni verrou d’écriture. Les modifications prennent directement le verrou du dossier de données, même avec un serveur actif ; les commandes de comptes ne passent jamais par la délégation.

## Exemples

Avec un fichier privé contenant une seule ligne, déjà préparé à `/private/path/account-password` :

```sh
aede accounts init operator --password-stdin < /private/path/account-password
aede accounts create alice user --password-stdin < /private/path/account-password
aede accounts list --json
aede accounts rename alice listener
aede accounts revoke listener
aede accounts disable listener
```

Employer des mots de passe forts distincts. Protéger le fichier de mot de passe et le supprimer lorsqu’il devient inutile. `accounts.json` conserve des vérificateurs Argon2id salés, jamais les mots de passe en clair. Sous Unix, ses permissions doivent interdire l’accès aux autres utilisateurs ; corriger un fichier accidentellement exposé avec `chmod 600` avant de réessayer. Des identifiants corrompus sont refusés plutôt que recréés.

## Résultat et erreurs

Une modification publie les identifiants atomiquement, sans changer l’audio ni `user.json`. Noms invalides, mots de passe courts, doublons, absence d’initialisation et retrait du dernier administrateur actif sont refusés. Les sauvegardes incluent les comptes ; les anciennes sauvegardes et [reset](reset.md) conservent les comptes existants. Restaurer les identifiants invalide toutes les sessions.

Continuer avec les [sessions et l’administration HTTP](../server/accounts.md), [serve](serve.md), [backup](backup.md) et [restore](restore.md).
