# accounts — Gérer les comptes, sessions et clés clients

Les comptes partagent le catalogue musical et conservent favoris, notes, étoiles, historique et collections sous leur propriétaire stable. Le premier administrateur récupère les données du propriétaire existant `local`. Renommer ou désactiver un identifiant conserve ses données personnelles. La CLI reste une interface de confiance du propriétaire système ; ses commandes personnelles existantes utilisent toujours `local`.

## Syntaxe et arguments

```text
aede accounts [list]
aede accounts init <name> [--password-stdin]
aede accounts create <name> <admin|user|auditor> [--password-stdin]
aede accounts password <name> [--password-stdin]
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

Les identifiants acceptent 1–64 lettres ASCII, chiffres, points, tirets ou tirets bas, sans distinction de casse. Une lettre ou un chiffre est obligatoire. Un mot de passe contient au moins 15 caractères Unicode et au plus 1024 octets UTF-8, sans NUL. Les espaces au début et à la fin sont conservés.

Dans un terminal, `init`, `create` et `password` demandent `Password:` puis `Confirm password:`. Rien ne s’affiche pendant la saisie ou le collage, pas même des astérisques. Appuyez sur Entrée après chaque saisie. Retour arrière retire le dernier caractère Unicode ; Ctrl-U efface la saisie. Ctrl-C, Ctrl-D ou Échap annulent sans sauvegarder. Une confirmation différente ou une longueur excessive conserve également les identifiants existants. La saisie au terminal accepte les caractères imprimables et refuse les caractères de contrôle. Les invites utilisent stderr : la sortie JSON sur stdout peut donc être redirigée. L’entrée standard et stderr doivent toutes deux être des terminaux pour la saisie masquée. Sur macOS et Linux, `stty` doit être disponible, comme pour les commandes de lecture locale ; Windows utilise sa console native. La fin d’un collage encore en attente est supprimée pendant que la saisie reste masquée, puis le mode initial du terminal est restauré avant la sauvegarde, y compris après une annulation ou une erreur de saisie. Si le nettoyage échoue, aucun identifiant n’est sauvegardé. Le message d’erreur indique si le terminal reste masqué pour protéger la saisie encore en attente.

Pour les scripts, utilisez explicitement `--password-stdin` avec une entrée redirigée. Ce mode accepte une ligne et retire un LF ou CRLF final ; les sauts de ligne internes sont refusés. Il ne demande pas de confirmation. Le drapeau refuse une entrée provenant d’un terminal pour éviter une saisie visible. Les mots de passe ne sont jamais des arguments de commande.

## Créer la clé avant de connecter un client Subsonic

**Le mot de passe du compte Aède ne permet pas de connecter Submariner ou un autre client Subsonic/OpenSubsonic. Créez d’abord une clé d’application :**

```sh
aede accounts keys alice create "Submariner Mac"
```

Remplacez le nom d’exemple `alice` par votre compte administrateur ou utilisateur existant et actif. Utilisez le même dossier `--data` que le serveur. Copiez uniquement la valeur complète après `API key (shown once):` : **129 caractères sous la forme `id.secret`**, avec le point et les deux parties de 64 caractères. Dans Submariner, collez-la dans **Password** ; l’ID seul du tableau ne suffit pas. Le secret complet n’est affiché qu’une fois et ne peut pas être retrouvé en listant les clés. Conservez-le dans un emplacement privé, ou créez une remplaçante en cas de perte. Voir la [configuration Submariner étape par étape](../server/subsonic.md), avec les chemins de l’exécutable et l’authentification par jeton à décocher.

Pour lister les clés ou en révoquer une :

```sh
aede accounts keys alice
aede accounts keys alice revoke <key-id>
```

La liste affiche uniquement des métadonnées ; la révocation retire seulement la clé choisie. Création et révocation sont prises en compte sans redémarrer le serveur. Les clés survivent au redémarrage, sans expiration automatique, et sont retirées par modification du compte, révocation globale du compte ou restauration. Le libellé est non blanc et limité à 128 octets UTF-8. Maximum : huit clés par compte, 512 au total. `--json` produit des tableaux de métadonnées pour liste/révocation, ou un objet de métadonnées avec `token` pour création. Gardez cette sortie de création privée. La liste ne demande pas de verrou ; création/révocation le prennent.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--password-stdin` | Lire une ligne de mot de passe depuis une entrée redirigée pour les scripts utilisant `init`, `create` ou `password`, sans confirmation. Omettre ce drapeau pour la saisie masquée au terminal. |
| `--json` | Les opérations de comptes donnent des tableaux de métadonnées sans mot de passe/vérificateur/jeton de session. Les clés utilisent les formes ci-dessus ; la création inclut le secret affiché une fois. |

La [référence des options](options.md) explique `--data`, couleurs et aide. La liste ne demande ni catalogue ni verrou d’écriture. Les modifications prennent directement le verrou du dossier de données, même avec un serveur actif ; les commandes de comptes ne passent jamais par la délégation. La saisie du mot de passe précède le verrouillage : attendre une réponse ne bloque donc pas les autres écritures. Les comptes sont ensuite relus sous verrou et les règles de modification sont vérifiées à nouveau avant publication.

## Exemples

Depuis un terminal, saisir et confirmer chaque mot de passe lorsque la commande le demande :

```sh
aede accounts init operator
aede accounts create alice user
aede accounts password alice
```

Pour un script, avec un fichier privé contenant une seule ligne, déjà préparé à `/private/path/account-password` :

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
