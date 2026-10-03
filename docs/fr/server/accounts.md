# Comptes, sessions et données privées

Les [clés OpenSubsonic](subsonic.md) persistantes se gèrent par CLI locale et authentifient uniquement `/rest` ; les routes natives gardent les sessions Bearer. Un changement de génération du compte, y compris les routes révoquant toutes ses sessions, retire aussi ses clés. Restaurer une sauvegarde contenant des comptes retire toutes les clés restaurées et change la génération globale ; une ancienne sauvegarde sans comptes conserve les identifiants et clés courants. Une simple déconnexion retire seulement sa session.

Initialiser les comptes avec [accounts](../cli/accounts.md). Sans magasin de comptes à la première lecture dans un nouveau processus HTTP local, les lectures du catalogue restent anonymes et l’administration locale utilise le secret facultatif `AEDE_ADMIN_TOKEN`. Dès que le processus a observé le magasin, chaque requête HTTP ou WebSocket `/api/v1` exige une session ou, en HTTP local, le jeton administratif. Un magasin ensuite absent ferme l’accès jusqu’à l’arrêt de ce processus ; redémarrer HTTP local sans magasin rétablit le mode de compatibilité anonyme. Des identifiants illisibles ou exposés ferment l’accès même à la première lecture. [HTTPS](remote.md) exige toujours des comptes et accepte seulement les sessions ; toutes les routes `/api/admin` y sont indisponibles.

Un `admin` gère les comptes et les tâches de l’installation. Un `user` lit et modifie uniquement ses propres données personnelles. Un `auditor` lit le catalogue partagé et emploie seulement `GET` ou `HEAD` sur ses propres données `/api/me/v1` ; toute modification personnelle, changement de mot de passe, administration ou tâche renvoie `403 forbidden`. Un auditeur peut toujours se connecter, consulter sa session et se déconnecter.

## Connexion et gestion d’une session

Les identifiants de connexion contiennent 1–64 lettres ou chiffres ASCII, points, tirets bas ou traits d’union, avec au moins une lettre ou un chiffre. Leur comparaison ignore la casse.

| Méthode et chemin | Corps | Résultat |
| --- | --- | --- |
| `POST /api/auth/v1/session` | `{"username":"alice","password":"…"}` | 200 `{token,token_type:"Bearer",expires_at,account}` |
| `GET /api/auth/v1/session` | Aucun | 200 métadonnées du compte |
| `DELETE /api/auth/v1/session` | Aucun | 204 ; révoquer cette session |
| `PUT /api/auth/v1/password` | `{"current_password":"…","new_password":"…"}` | 204 ; révoquer toutes les sessions de ce compte (`auditor` : 403) |

`expires_at` est l’expiration absolue en secondes Unix. Envoyer exactement un en-tête `Authorization: Bearer <token>` ensuite. Ni paramètre d’URL, cookie ou jeton administratif pour `/api/auth/v1` et `/api/me/v1`. Ne jamais placer de secret dans une URL, argument, journal ou stockage local du navigateur. Un fichier JSON protégé peut fournir le corps de connexion avec `curl --data-binary @/private/path/login.json` ; protéger aussi la sortie contenant le jeton. Cette API vise les clients locaux explicites ; aucune page de connexion ni session par cookie actuellement.

Les jetons contiennent 256 bits provenant du générateur aléatoire système et restent en mémoire du processus. Ils expirent après 12 heures ou 30 minutes sans requête authentifiée du client. Les notifications serveur ne prolongent pas l’activité. Déconnexion, redémarrage, changement de mot de passe/rôle/nom/état et révocation invalident les sessions ; restaurer les identifiants renouvelle l’époque du magasin. Les WebSockets vérifient la session avant envoi et chaque seconde, puis ferment les sessions invalides. Maximum 256 sessions et huit par compte ; une neuvième remplace la plus ancienne de ce compte. Capacité globale atteinte : `429 session_limit`.

Deux travailleurs bornés vérifient les mots de passe. Limites : cinq tentatives par identifiant et 100 au total par minute, réussites incluses ; `429 login_limited` inclut `Retry-After: 60`. Identifiant inconnu, compte désactivé et mauvais mot de passe donnent le même `401 invalid_credentials`. Authentification occupée : `503 authentication_busy` ; identifiants illisibles : `503 accounts_unavailable`. Les corps gardent la limite de 16 Kio et le délai d’une seconde, exigent un objet JSON et refusent champs/paramètres inconnus. Les réponses portent `Cache-Control: no-store`.

## Ses annotations, historiques et collections

Employer les [routes personnelles](personal.md) avec `/api/me/v1` au lieu de `/api/admin/v1`. Sélecteurs, corps, pagination et réponses restent identiques. Le serveur déduit le propriétaire depuis la session ; aucune requête ne peut le choisir. Cela inclut compteurs d’écoute et contenu des collections. Un `user` ou un `admin` peut employer toutes les méthodes listées pour ce propriétaire. Un `auditor` peut employer seulement `GET` et `HEAD` ; les modifications renvoient `403 forbidden`. Deux travailleurs bloquants partagés avec les listes/détails du catalogue, la navigation et l’inspection bornent les opérations personnelles ; saturation des travailleurs : `503 personal_busy`, contention du verrou d’écriture : `409 store_busy`. Chaque opération utilise catalogue et données personnelles actuels sous le verrou, puis revérifie la session avant lecture ou modification.

| Route | Méthodes |
| --- | --- |
| `/api/me/v1/annotation` | GET/HEAD/PUT par `ref` |
| `/api/me/v1/history` | GET/HEAD événements paginés ; POST une écoute |
| `/api/me/v1/collection` | GET/HEAD/PUT/DELETE par `name` |
| `/api/me/v1/collections` | GET/HEAD collections paginées |
| `/api/me/v1/playback` | GET avec passage en WebSocket ; [contrat audio](playback.md), user/admin seulement |

Les routes transitoires `/api/admin/v1/{annotation,history,collection,collections}` continuent à viser `local`, même pour un administrateur ayant un autre propriétaire. Le premier administrateur récupère `local`. La CLI reste une interface de confiance du propriétaire système et conserve sa portée locale.

## Administration des comptes

En HTTP local, les administrateurs peuvent lancer les [tâches de l’installation](jobs.md) et gérer les comptes ; un `user` ou un `auditor` reçoit `403 forbidden` sur l’administration. HTTPS désactive ces routes pour tous ; employer la CLI locale. Un administrateur actif doit toujours rester. Employer une session administrateur ou le jeton administratif historique facultatif :

| Méthode et chemin | Corps | Résultat |
| --- | --- | --- |
| `GET /api/admin/v1/accounts` | Aucun | 200 tableau de métadonnées |
| `POST /api/admin/v1/accounts` | `{username,password,role}` ; rôle `admin`, `user` ou `auditor` | 201 métadonnées du nouveau compte |
| `GET /api/admin/v1/accounts/{username}` | Aucun | 200 métadonnées |
| `PATCH /api/admin/v1/accounts/{username}` | Au moins un champ parmi `username,role,enabled,password` | 200 métadonnées modifiées ; sessions concernées révoquées |
| `DELETE /api/admin/v1/accounts/{username}/sessions` | Aucun | 204 ; révoquer toutes les sessions de ce compte |

Champs publics : `id,username,role,enabled,created_at,updated_at`, sans vérificateur, génération de session ni secret. PATCH refuse null, champs inconnus et modifications invalides sans publication partielle. Opération invalide : `400 account_refused` ; compte absent : `404 account_not_found`. Initialisation et récupération restent dans la CLI locale ; aucune inscription publique, suppression de compte ou récupération par e-mail. La [conception des comptes](../../design/accounts.md) décrit stockage privé et compatibilité des sauvegardes.
