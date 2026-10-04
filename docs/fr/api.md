<div id="aède-http-api-v1-m2" data-legacy-anchor></div>

# API HTTP d’Aède v1 (M2)

Pour créer ou adapter un lecteur, commencez par le [cahier des charges Compatible Aède](server/compatible-aede.md) : il définit les profils catalogue, lecteur natif et adaptateur, leurs exigences et tests d’acceptation. Cette référence API précise leur contrat d’échange.

L’[adaptateur Subsonic/OpenSubsonic](server/subsonic.md) utilise séparément `/rest`, des clés API révocables et des enveloppes XML/JSON. Il ajoute la navigation ID3, la diffusion des originaux, les pochettes présentes à côté des fichiers et les favoris, notes, playlists ordonnées et écoutes déclarées privés, sans changer le contrat natif v1 ci-dessous. Seule la découverte d’extensions est anonyme ; l’accès distant conserve la frontière HTTPS/Host/Origin. Le guide propose un premier essai sur macOS avec Supersonic.

Ce document définit le contrat client figé du catalogue en lecture seule et de ses interfaces de comptes/données personnelles ajoutées. Toute divergence entre ce contrat et l’implémentation est un défaut. Il est indépendant des valeurs `format_version` des fichiers sur disque. Le préfixe `/api/v1` fige les noms, types et sens des champs existants ; des ajouts compatibles peuvent introduire des champs facultatifs ou de nouvelles routes. Supprimer ou réinterpréter un champ, modifier le type d’un champ obligatoire ou changer le sens des paramètres impose un nouveau préfixe. Les clients doivent ignorer les champs de réponse inconnus et prendre leurs décisions à partir de `error.code`, pas de `error.message`.

Démarrez avec `aede serve [--port N]` après `aede scan <folder>`. L’adresse par défaut est `127.0.0.1:8787` ; `--port 0` demande un port local disponible au système et l’affiche. Le processus lit le même catalogue JSON que la CLI. Le démarrage sans catalogue échoue. Il ne modifie jamais les fichiers audio ni leurs tags.

Le [README du serveur](../../crates/aede-server/README.md) constitue la référence complète des routes et paramètres, y compris les ajouts de navigation proches de la CLI et les tâches administratives décrites ci-dessous. Les nouvelles réponses qu’il documente font partie du contrat v1. Redémarrez le serveur après une mise à jour de l’exécutable pour utiliser les nouvelles routes.

<div id="access-boundary" data-legacy-anchor></div>

## Frontière d’accès

Toutes les routes `/api/v1`, y compris `/status` et le WebSocket, sont en lecture seule. Sans comptes, elles restent anonymes et peuvent révéler noms, chemins, commentaires et origines aux autres utilisateurs/processus locaux. Avec des [comptes](server/accounts.md), une session ou le jeton administratif est exigé ; `/api/me/v1` limite les opérations personnelles au propriétaire authentifié. Une session `auditor` peut seulement employer ses routes personnelles `GET`/`HEAD`, tandis que la gestion des comptes exige un administrateur. Aucune annotation privée ni clé de service dans les réponses du catalogue. Dès qu’un processus en cours a observé le magasin de comptes, des identifiants absents ou illisibles ferment l’accès jusqu’à son arrêt. Un nouveau processus HTTP local démarré sans magasin reprend le mode de compatibilité anonyme ; HTTPS exige toujours des comptes initialisés. Un magasin illisible ferme l’accès même à la première lecture.

HTTP écoute par défaut sur la boucle locale. Une [écoute HTTPS explicite](server/remote.md) exige des comptes, certificat/clé et une adresse publique configurée. En HTTPS, catalogue et données personnelles exigent une session, le jeton historique est désactivé et toutes les routes `/api/admin` sont indisponibles. Aucune redirection automatique, autorisation d’origine tierce ou interface de connexion de navigateur n’est implémentée. Les routes personnelles administratives transitoires conservent `local` en HTTP local.

Chaque requête HTTP et négociation WebSocket fournit un seul `Host` correspondant à l’adresse autorisée. HTTP par défaut accepte `127.0.0.1` ou `localhost` avec le port réel (80 peut être omis) ; une IP locale explicite utilise son adresse réelle. HTTPS accepte seulement son `HOST:PORT` configuré. Adresse absente, répétée, mal formée ou étrangère : `403 invalid_host` ou erreur d’analyse HTTP. Les cibles absolues emploient le schéma du transport et la même adresse. Les clients natifs peuvent omettre `Origin` ; si présente, une unique origine correspondante en HTTP ou HTTPS respectivement est exigée. Origine étrangère, null, mal formée ou répétée : `403 invalid_origin`. Le jeton administratif historique refuse aussi une origine correspondante.

<div id="transport-and-representations" data-legacy-anchor></div>

## Transport et représentations

Les réponses aux requêtes réussies et aux erreurs applicatives sont du JSON UTF-8 avec `Content-Type: application/json`. `GET` est défini ; les routes JSON ordinaires acceptent aussi `HEAD`, avec mêmes statut/en-têtes mais sans corps. Un chemin inconnu renvoie une erreur JSON 404 ; une méthode différente sur un chemin connu renvoie une erreur JSON 405. La négociation WebSocket est une exception : les échecs de passage en WebSocket sont des erreurs de transport, sans garantie d’enveloppe JSON. Aucune méthode d’écriture n’est définie sous `/api/v1`.

Tous les noms de champs suivent `snake_case`. Les dates `scanned_at` sont en secondes Unix ; durées en millisecondes, tailles en octets. Les valeurs scalaires facultatives sont `null`, pas absentes. Les collections sont des tableaux, même vides. Noms et titres conservent l’écriture lue dans les fichiers. Les relations utilisent les jetons stables `reference` d’`EntityRef`, jamais les indices des tableaux du catalogue. Une référence de piste dépend de son chemin et change lorsque le fichier est déplacé. Les clients doivent encoder le jeton pour URL lorsqu’ils le transmettent comme paramètre.

Les routes d’origine `/status` et `/library` lisent les métadonnées et compteurs de l’instantané en temps constant et n’interprètent pas les paramètres de requête. Elles restent hors du budget des travailleurs d’inspection du catalogue ; n’ajoutez aucun paramètre sur ces deux routes. L’authentification et l’admission du transport s’appliquent toujours.

| Requête | Réponse |
| --- | --- |
| `GET /api/v1/status` | `{ "status": "ok", "api_version": 1, "catalog_loaded": bool }`, y compris si le catalogue disparaît après le démarrage du serveur. |
| `GET /api/v1/library` | `{ "scanned_at": u64, "files": usize, "artists": usize, "releases": usize, "recordings": usize, "tracks": usize }`. |
| `GET /api/v1/artists` | Page de résumés d’artistes. |
| `GET /api/v1/releases` | Page de résumés de sorties/éditions. |
| `GET /api/v1/tracks` | Page de résumés de pistes. |
| `GET /api/v1/recordings` | Page de résumés d’enregistrements. |
| `GET /api/v1/entities?ref=<token>` | Détail d’une entité sélectionnée par un jeton stable. |
| `GET /api/v1/lyrics?track=<token>` | Paroles locales complètes avec horodatages éventuels, ou `lyrics: null`. |
| `GET /api/v1/events` | Notifications WebSocket de catalogue, inchangées par rapport au contrat v1 figé. |
| `GET /api/v1/activity` | Notifications WebSocket de catalogue et d’activité des tâches. |

Chaque liste possède `{ "items": [...], "total": usize, "offset": usize, "limit": usize, "scanned_at": u64 }`. `total` compte les lignes **après** recherche et filtres, avant pagination. `items` est découpé après tri. `offset` et `limit` rappellent les valeurs réellement utilisées. Une page au-delà de la fin contient `items: []` et le `total` filtré. Tous les champs des tableaux de résumés/détails suivants sont obligatoires dans leur structure, même lorsqu’une valeur est `null` ou un tableau vide.

| Résumé | Champs |
| --- | --- |
| Artiste | `reference: string`, `name: string`, `sort_name: string`, `mbid: string|null`, `aliases: string[]`. |
| Sortie | `reference: string`, `title: string`, `year: u32|null`, `album_artist: reference|null`, `track_count: usize`, `cover_path: string|null`. |
| Piste | `reference: string`, `title: string`, `release: reference|null`, `recording: reference|null`, `duration_ms: u64|null`. |
| Enregistrement | `reference: string`, `title: string`, `mbid: string|null`, `track_count: usize`, `work_count: usize`. |

`/entities` renvoie un objet avec `kind`, `reference` et les champs ci-dessous. Les types reconnus sont `artist`, `release`, `track`, `recording`, `work`, `release_group`, `label` et `genre`.

| Type | Champs supplémentaires |
| --- | --- |
| `artist` | `name`, `sort_name`, `mbid`, `aliases`, `releases: reference[]`. |
| `release` | `title`, `year`, `album_artist: reference|null`, `tracks: reference[]`, `release_group: reference|null`, `labels: reference[]`, `cover_path: string|null`. |
| `track` | `title`, `release: reference|null`, `recording: reference|null`, `duration_ms: u64|null`, `path: string`, `size: u64` ; les serveurs récents ajoutent `analyses: object[]`. |
| `recording` | `title`, `isrc: string|null`, `mbid: string|null`, `tracks: reference[]`, `works: reference[]`. |
| `work` | `title`, `mbid: string`, `recordings: reference[]`. |
| `release_group` | `title`, `mbid: string`, `releases: reference[]`. |
| `label` | `name`, `mbid: string|null`, `releases: reference[]`. |
| `genre` | `name`, `releases: reference[]`, `tracks: reference[]`. |

Les tableaux de références d’un détail d’entité sont complets, sans pagination. Pour une simple liste, utilisez une route paginée avec ses filtres. Une future sous-ressource de relations paginées pourra être ajoutée sans modifier ces champs v1.

Chaque analyse de piste contient `source`, `source_version`, `imported_at`, `stale`, les champs de mesure déjà enregistrés et `source_data`. `source_data` est l’entrée complète du fichier source lorsqu’elle est conservée, ou `null` pour les anciens imports ; les autres champs de mesure continuent de décrire ces imports. Les analyses restent attribuées à leur source, sans fusion silencieuse avec les faits locaux du fichier. `/api/v1/track` et `/api/v1/entities` renvoient la même structure de détail de piste.

<div id="cli-shaped-additions" data-legacy-anchor></div>

### Ajouts proches de la CLI

`/albums` expose des filtres d’artiste/genre/label/nom/année proches de la CLI, tandis que `/releases` conserve le sens de ses paramètres d’origine. Les routes au singulier `/album`, `/artist`, `/track`, `/recording`, `/work`, `/release-group`, `/genre` et `/label` choisissent exactement une entité avec `ref` ou `name`. Une correspondance exacte normalisée passe avant une correspondance partielle ; une ambiguïté produit `409 ambiguous_entity`, avec au plus 200 candidats `{reference,name}` dans `error.candidates`, jamais un premier résultat arbitraire. Ces détails suivent les champs d’entité ci-dessus ; `/artist` ajoute `origin`. `/from` renvoie seulement référence, nom et origine de cet artiste, explicitement `known` ou `unknown`, avec message explicatif et attribution. Seuls des faits MusicBrainz fiables déjà enregistrés établissent l’origine.

`/genres`, `/labels`, `/works`, `/release-groups`, `/countries`, `/years` et `/roles` ajoutent une navigation paginée. `/doctor`, `/stats` et `/roots` fournissent diagnostics/inspection structurés ; `/search` classe les noms et éventuellement les commentaires enregistrés ; `/query` évalue le sous-ensemble public de la grammaire CLI. Prédicats/tris dépendant du propriétaire et requêtes de paroles sont refusés. Aucune lecture ne télécharge d’information externe. Voir les [paramètres et structures complets](../../crates/aede-server/README.md#read-routes).

Les listes et détails du catalogue, y compris les routes d’origine `/artists`, `/releases`, `/tracks`, `/recordings` et `/entities`, partagent deux emplacements bornés de travail bloquant avec la navigation, l’inspection et les opérations personnelles. À saturation, catalogue/navigation/inspection renvoient `429 inspection_busy` ; les opérations personnelles renvoient `503 personal_busy`. L’analyse d’expression est limitée à 64 unités de complexité. HTTPS ajoute un budget d’admission de requêtes distinct, déplace les traitements hors des travailleurs du moteur asynchrone et fixe un délai de réponse ; la capacité sur cible reste à mesurer. Doctor lit catalogue/conclusions et sources actuels sous verrou partagé (`409 store_busy` s’il est indisponible) ; les autres routes utilisent le catalogue en cache et, au besoin, le dernier fichier de sources sauvegardé atomiquement. Une source corrompue produit une erreur, pas une origine inconnue trompeuse. Aucun instantané commun à plusieurs requêtes ou au couple catalogue/sources n’est garanti pour ces dernières lectures.

<div id="search-filters-sorting-and-pagination" data-legacy-anchor></div>

## Recherche, filtres, tri et pagination

Ces paramètres concernent les quatre listes d’origine. Paramètres inconnus/répétés, `q` vide, tris non pris en charge et références mal formées sont des erreurs. Recherche et filtres se combinent avec ET ; chaque ligne correspondante apparaît une fois. Le texte utilise la comparaison normalisée d’Aède, sans distinction de casse/accents, par fragment. La recherche examine seulement les champs suivants, sans consulter silencieusement d’autres tags ou sources externes.

Les valeurs `q`, `name` et `mbid` du catalogue sont limitées à 2048 octets UTF-8 après décodage de l’URL et avant normalisation. Les références stables, dont `/entities?ref=…` et les filtres par référence, sont limitées à 16384 octets UTF-8. Lorsqu’un sélecteur accepte un nom ou une référence, les noms utilisent la limite de 2048 octets et les préfixes de types de référence reconnus celle de 16384 octets ; la référence doit toujours avoir le type exigé par la route. Une valeur trop longue renvoie `400 invalid_query`.

| Liste | Champs examinés par `q` | Filtres exacts | `sort` accepté |
| --- | --- | --- | --- |
| `/artists` | `name` et `aliases` | `mbid=<string>` | `catalog` par défaut, `name` selon `sort_name`. |
| `/releases` | `title` | `year=<u32>`, `artist=<artist reference>` | `catalog` par défaut, `title`, `year`. |
| `/tracks` | `title` | `release=<release reference>` | `catalog` par défaut, `title`. |
| `/recordings` | `title` | `work=<work reference>` | `catalog` par défaut, `title`. |

`order=asc|desc` vaut `asc` par défaut et s’applique à tout tri accepté. `catalog` est l’ordre déterministe enregistré au scan. Le tri textuel compare le texte normalisé, puis l’ordre du catalogue pour les égalités. Le tri `year` place les années inconnues à la fin dans les deux sens ; les années égales sont départagées par titre normalisé, puis ordre du catalogue. Les filtres de références exigent le type attendu. Une référence valide mais absente produit `404 entity_not_found` ; mal formée ou du mauvais type, `400 invalid_query`. Un filtre exact `mbid` distingue la casse.

Le filtre `artist` des sorties correspond à l’artiste d’album, pas à chaque artiste crédité. `release` des pistes correspond à leur album. `work` des enregistrements correspond à une œuvre explicitement identifiée. `year` est l’année enregistrée de la sortie et `mbid` l’identifiant enregistré de l’artiste. Aucun filtre ne consulte les informations téléchargées ni les annotations privées.

`offset` vaut 0 par défaut. `limit` vaut 50 et doit être entre 1 et 200. Ce sont des entiers décimaux non négatifs, sans signe ni espace. Recherche/filtres précèdent le tri, puis le découpage de page. Le catalogue peut changer entre requêtes : les clients comparant les pages doivent comparer `scanned_at` et recommencer la pagination après un changement. Aucun instantané commun aux requêtes n’est garanti.

<div id="errors-and-versioning" data-legacy-anchor></div>

## Erreurs et versions

Une erreur applicative est `{ "error": { "code": string, "message": string } }`. `code` est stable en v1 ; `message` s’adresse aux personnes et peut changer. Résultats définis :

| HTTP | `error.code` | Sens |
| --- | --- | --- |
| 400 | `invalid_query` | Paramètre de recherche/filtre/tri/ordre inconnu, répété ou invalide. |
| 400 | `invalid_pagination` | `offset` ou `limit` invalide. |
| 400 | `invalid_reference` | Référence `/entities` absente ou mal formée. |
| 403 | `invalid_host` | Autorité de requête différente de cette écoute locale, ou ambiguë. |
| 403 | `invalid_origin` | Origine de navigateur étrangère, mal formée ou ambiguë. |
| 404 | `entity_not_found` | Référence correctement formée absente du catalogue actuel. |
| 404 | `not_found` | Chemin inconnu. |
| 404 | `source_unavailable`, `lyrics_unavailable` | Le fichier audio ou de paroles catalogué est indisponible. |
| 405 | `method_not_allowed` | Méthode HTTP non prise en charge sur un chemin connu. |
| 409 | `ambiguous_entity` | Un nom au singulier choisit plusieurs entités ; des candidats bornés accompagnent l’erreur. |
| 409 | `store_busy` | Doctor ou le scan administratif synchrone ne peut prendre le verrou des données. |
| 409 | `source_changed`, `lyrics_changed` | Les paroles ne peuvent être associées aux fichiers audio/paroles réguliers actuels ; restaurer ou rescanner la source. |
| 413 | `lyrics_too_large` | Les paroles complètes dépassent une limite d’entrée, de développement ou de réponse JSON. |
| 429 | `inspection_busy` | Le budget partagé de catalogue/navigation/inspection est plein. |
| 500 | `sources_unavailable` | Un fichier de sources enregistré n’a pas pu être lu. |
| 500 | `catalog_read_failed` | Doctor n’a pas pu lire catalogue/conclusions. |
| 500 | `inspection_failed` | Un travail d’inspection en arrière-plan a échoué. |
| 500 | `lyrics_read_failed` | La lecture complète des paroles a échoué. |
| 503 | `catalog_unavailable` | Le catalogue a été retiré pendant le fonctionnement du serveur. |
| 503 | `connection_limit` | La limite partagée de connexions WebSocket est atteinte. |
| 503 | `lyrics_timeout` | La lecture locale complète des paroles a dépassé son délai de réponse. |

Le serveur vérifie `catalog.json` environ chaque seconde. Un remplacement réussi échange l’instantané en mémoire. Un remplacement illisible laisse l’ancien instantané en service et écrit une erreur sur la sortie d’erreur ; supprimer le fichier fait renvoyer 503 aux routes de catalogue jusqu’au chargement d’un nouveau catalogue. `/status` reste disponible. Les échecs inattendus de transport/exécution sortent de l’enveloppe d’erreur applicative.

Le WebSocket `/api/v1/events` envoie immédiatement `{ "type": "snapshot", "scanned_at": u64|null }`, puis `{ "type": "catalog_changed", "scanned_at": u64|null }` après remplacement réussi ou retrait. `null` signifie aucun catalogue chargé. Ce flux figé n’ajoute pas de types de tâches ni de messages d’erreur. Il ne transmet pas de mise à jour partielle du graphe : les clients relisent les pages HTTP. Les notifications sont fournies sans garantie ; une reconnexion reçoit un nouveau snapshot, sans certitude de recevoir chaque changement intermédiaire. Le socket n’accepte aucune commande.

Les deux flux partagent une limite de 64 WebSockets ouverts. Une ouverture supplémentaire produit `503 connection_limit` jusqu’à fermeture d’une connexion ; les lectures HTTP ordinaires restent disponibles. Trames/messages entrants limités à 1 Kio ; tout texte ou binaire applicatif ferme la connexion, tandis que ping/pong/close standards restent acceptés. Un envoi qui ne finit pas en cinq secondes ferme sa connexion. HTTPS borne aussi les requêtes à deux travailleurs de fond, avec un délai de réponse de quinze secondes ; un travail continuant après ce délai garde sa place. Ces protections demandent encore une validation de capacité sur cible.

<div id="activity-stream" data-legacy-anchor></div>

## Flux d’activité

`GET /api/v1/activity` est un WebSocket distinct pour suivre les tâches et changements de catalogue. Il commence avec le même `snapshot` et inclut `catalog_changed` avec le même sens que `/api/v1/events`. Il ajoute ces messages JSON :

| `type` | Champs | Sens |
| --- | --- | --- |
| `task_started` | `task_id: u64`, `task_kind: string` | Une tâche acceptée commence ; une commande déléguée peut encore attendre le verrou des données. |
| `task_progress` | `task_id`, `task_kind`, `phase: string`, `done: usize`, `total: usize` | Progression de la tâche. |
| `task_completed` | `task_id`, `task_kind`, `scanned_at: u64|null` | Opération terminée. Le serveur tente un rechargement avant ce message ; avec un autre verrou, `catalog_changed` peut suivre plus tard. |
| `task_failed` | `task_id`, `task_kind`, `code: string`, `message: string` | Échec ; aucun message de fin réussie ne suit. |
| `error` | `operation: string`, `code: string`, `message: string` | Échec d’un travail sans ID, actuellement un rechargement du catalogue. |

`task_id` est unique dans un processus serveur, sans persistance ni continuité entre redémarrages ; ce n’est pas un identifiant de catalogue. Les familles sont `scan`, `identification` pour un fetch CLI délégué ou HTTP asynchrone, et `command` pour les autres modifications CLI déléguées. Un scan administratif synchrone émet `task_started`, `task_progress` (phase `discovered`, puis `reading` si des fichiers demandent de nouveaux tags), `catalog_changed`, puis `task_completed`. En `discovered`, done/total valent tous deux le nombre de fichiers audio trouvés. En `reading`, ils comptent les fichiers nécessitant une lecture fraîche des tags, pas tous les fichiers découverts. Les mises à jour de lecture sont limitées à environ quatre par seconde, avec valeur finale transmise. Les commandes CLI déléguées et tâches HTTP asynchrones émettent aussi `running` avant leur exécution et `refreshing` avant publication du catalogue résultant. Ces phases utilisent `done: 0, total: 0` : aucun total fiable par élément n’est disponible. Le détail reste dans la CLI connectée ou le résultat HTTP authentifié. Une tâche déléguée peut attendre le verrou après `task_started`. Une requête refusée, notamment `401 unauthorized` ou `409 store_busy`, ne commence pas de tâche et n’émet aucun message de tâche.

Les codes actuellement émis sont `scan_failed`, `store_error` ou `catalog_unavailable` pour un scan administratif en échec, `command_failed` pour une commande déléguée ou tâche HTTP, `task_cancelled` pour un scan/fetch délégué ou HTTP arrêté par l’utilisateur, et `catalog_reload_failed` pour un rechargement en arrière-plan. `message` sert à l’affichage, pas aux décisions logicielles. Les clients doivent ignorer types de messages, familles de tâches, phases et champs inconnus pour autoriser les ajouts compatibles. Ce flux est sans garantie ni rejeu : à la reconnexion, le client reçoit un nouvel état du catalogue, pas l’historique des tâches ni leur statut actuel garanti. Aucun WebSocket n’accepte de commandes. Aucun message ne contient le catalogue complet ; relisez HTTP après `catalog_changed`.

Le préfixe `/api/v1` et `api_version: 1` sont indépendants des versions du programme et des stores JSON. Des routes/champs facultatifs peuvent être ajoutés en v1. Champs obligatoires, codes d’erreur et sens des requêtes existants restent stables. Une rupture reçoit `/api/v2` ; les anciens clients continuent avec v1 tant qu’elle est servie.

<div id="administrative-work-separate-opt-in-api" data-legacy-anchor></div>

## Travail administratif — API distincte, activée explicitement

`POST /api/admin/v1/scan` **sans corps** conserve le comportement synchrone d’origine : rescan des racines suivies, avec fusions d’artistes enregistrées et conclusions indépendantes, puis `{ "status": "completed", "scanned_at": u64, "files": usize }` après sauvegarde/publication. Le verrou d’écriture reste tenu jusqu’à publication. Une écriture concurrente produit `409 store_busy` ; échec de scan/stockage : `500 scan_failed` ou `500 store_error`.

Un objet de corps, même `{}`, choisit le nouveau scan asynchrone. `POST /api/admin/v1/fetch` accepte également un objet et lance une tâche typée. Les champs correspondent aux options CLI : dossiers/full/threads de scan et passes/cibles explicites de fetch. Une soumission acceptée renvoie `202 {task_id,status:"queued",status_url}`. `GET /api/admin/v1/tasks/{id}` donne statut actuel et sortie bornée ; `POST /api/admin/v1/tasks/{id}/cancel` sans corps demande l’annulation. Les tâches attendent le verrou d’écriture existant, survivent à la déconnexion HTTP et sont attendues à l’arrêt. Quatre au maximum sont actives ; 64 dossiers sont conservés en mémoire, les terminés évincés en premier. Aucun historique ne survit au redémarrage. Suivi/annulation HTTP ne concernent ni les tâches CLI déléguées ni les scans synchrones. Voir la [référence administrative](../../crates/aede-server/README.md#administrative-routes) pour champs, états, sorties et limites.

L’administration des tâches refuse tout paramètre d’URL (`400 invalid_query`). Corps objet limité à 16 Kio et reçu en une seconde ; champs mal formés/inconnus : `400 invalid_body`, erreurs sémantiques d’options : `400 invalid_parameters`, délai : `408 request_timeout`. Capacité épuisée : `503 task_limit`, tâches absentes/évincées : `404 task_not_found`, ID invalide : `400 invalid_task_id`, annulation de tâche terminée : `409 task_not_cancellable`. Échec du registre : `500 task_error`. Annuler ne supprime pas la progression déjà enregistrée. Aucun exécutable arbitraire, liste d’arguments, remplacement de dossier de données ou d’identifiant n’est accepté.

Les routes personnelles administratives conservent `local` ; les [sessions de comptes](server/accounts.md) emploient les mêmes opérations sous `/api/me/v1`, liées à leur propriétaire authentifié. La [référence du serveur](../../crates/aede-server/README.md#personal-data-annotations-plays-and-smart-collections) définit sélecteurs et JSON. Aucun argument de propriétaire accepté. Les annotations règlent favoris, étoiles 1–5, notes et tags sans modifier l’audio. Les opérations chargent catalogue et données actuels sous verrou, revérifient la session et conservent le verrou jusqu’à la publication atomique. Conflit : `409 store_busy`, travailleurs occupés : `503 personal_busy`, données illisibles : `500 user_unavailable`. Historique conservé par date, soumissions retardées comprises ; les compteurs totaux incluent les événements hors journal retenu. Les playlists persistantes ordonnées sont disponibles sous `/rest`, sans route native v1 équivalente pour l’instant. Les annotations de relations HTTP restent futures.

Une session administrateur autorise ces routes. Le jeton historique `AEDE_ADMIN_TOKEN` reste facultatif : secret ASCII privé d’au moins 32 caractères, configuré avant démarrage. Sans comptes ni jeton, administration indisponible. Chaque requête utilise un seul `Authorization: Bearer <token>` ; absence/doublon/erreur : JSON `401 unauthorized`, compte `user` ou `auditor` : `403 forbidden`. Aucun secret en URL. Le jeton historique refuse toute `Origin` ; les sessions respectent la vérification d’origine locale commune. Les tâches utilisent les droits du compte système serveur : l’administrateur reste de confiance. La [référence des comptes](server/accounts.md) définit connexion, changement de mot de passe, gestion, expiration et limites.

Sur Unix, les commandes CLI pouvant écrire les stores se délèguent automatiquement au serveur actif du même dossier de données. Le socket Unix privé est distinct de HTTP et de tout accès distant. Le serveur exécute indépendamment, transmet l’affichage habituel à la CLI et laisse finir même si elle se déconnecte. Cela compte pour un long fetch, qui sauvegarde après chaque réponse. Scan/fetch délégués affichent leur ID ; `aede cancel <task-id>` demande à ce serveur de les arrêter. L’annulation répond immédiatement et la commande originale sort avec code 130 une fois arrêtée. Tâche déjà terminée, scan HTTP administratif et autres commandes ne s’annulent pas par cette commande. L’ID est valable dans ce processus seulement. Fermer la CLI, même par Ctrl-C, n’annule pas la tâche. Les JSON déjà sauvegardés restent ; un téléchargement interrompu peut laisser un temporaire caché, jamais image/paroles finales tronquées. Le socket est dans un dossier privé mode 0700 sous `/tmp`, adapté aux longs chemins de données ; les données ne doivent être accessibles en écriture ni au groupe ni aux autres. Aucun jeton administratif n’est nécessaire pour ce canal du même compte. Sans serveur, les commandes restent locales et cancel refuse. Sous Windows, délégation/annulation locales sont indisponibles ; la CI native vérifie scan, fichiers annexes et copies (voir [Chemins](../design/paths.md)).

Le fichier exclusif `.aede.lock` reste la protection finale contre les mises à jour perdues : sous-processus délégués et CLI sans serveur le tiennent pendant toute lecture/modification/écriture ; backup le tient pour un instantané cohérent entre fichiers. Le scan administratif synchrone renvoie 409 au lieu d’attendre ; les tâches HTTP asynchrones attendent. Conservez ce fichier même à l’arrêt : le retirer pendant sa détention peut neutraliser le verrou. Le remplacement JSON atomique protège des fichiers partiels. C’est une coopération entre Aède actuels ; éditer manuellement le JSON ou utiliser simultanément un ancien exécutable reste dangereux. Le serveur recharge les changements de catalogue CLI environ chaque seconde.

Ctrl-C/SIGTERM arrêtent les nouvelles connexions et ferment les WebSockets actifs. Les scans/fetch acceptés terminent avant la sortie ; annulez une tâche asynchrone indésirable avant l’arrêt. HTTP par défaut reste local ; un client distant doit employer HTTPS explicitement configuré avec comptes.

## Paroles locales

`GET /api/v1/lyrics?track=<référence stable de piste>` est un ajout en lecture seule, sans pagination. Paramètres absents, dupliqués, inconnus et références d’un autre type sont refusés. Une piste actuelle sans paroles non vides répond `{ "track": "track:…", "lyrics": null }` ; sinon la réponse complète est :

```json
{"track":"track:…","lyrics":{"source":"sidecar","synced":true,"lines":[{"at_ms":1250,"text":"Première ligne"},{"at_ms":2500,"text":""},{"at_ms":null,"text":"Couplet sans horodatage"}]}}
```

`source` vaut `tag` ou `sidecar` ; les paroles non vides du tag précèdent le `.lrc` adjacent catalogué. Aucun chemin d’origine supplémentaire n’est fourni. `synced` indique qu’au moins une ligne possède un horodatage. `at_ms` est une position entière non négative en millisecondes depuis le début du morceau, ou `null` ; le décalage LRC a déjà été appliqué. `text` est du texte UTF-8 littéral, avec remplacement des octets invalides du fichier de paroles. Ordre source, horodatages répétés et lignes vides horodatées sont conservés. Les lignes horodatées peuvent être désordonnées ; le client regroupe les horodatages égaux, puis choisit le dernier groupe chronologique ne dépassant pas sa propre position audio consommée. Les lignes sans horodatage restent lisibles, sans timing inventé. Voir les [règles d’horloge du client](server/playback.md#paroles-et-horloge-du-client).

L’authentification du catalogue s’applique, y compris aux auditeurs en lecture seule, et est vérifiée de nouveau avant publication. Seul le catalogue actuel sélectionne une source ; aucun chemin arbitraire n’est accepté. Taille, date précise de modification de l’audio et métadonnées des descripteurs/chemins ouverts sont contrôlées ; un ancien catalogue avec dates à la seconde exige un scan normal. Le fichier de paroles doit porter le même nom de base et se trouver dans le même dossier que l’audio, avec extension `.lrc` insensible à la casse. Liens, fichiers non réguliers et sources modifiées sont refusés. L’entrée est limitée à 256 Kio, le texte complet décodé/développé à 1 Mio et le JSON sérialisé à 1 Mio, échappement et métadonnées des lignes compris. Un contenu excessif produit une erreur explicite, jamais une troncature silencieuse. Les travailleurs d’inspection partagés bornent lecture et sérialisation, avec un délai de réponse de dix secondes ; une opération bloquante conserve son emplacement jusqu’à sa fin, même après expiration du délai ou déconnexion. Les réponses utilisent `Cache-Control: no-store`.

Cette route ne télécharge et n’écrit rien, et fonctionne indépendamment de l’audio. Les messages PCM et méthodes Subsonic/OpenSubsonic restent inchangés. Un client comme Phémios peut récupérer les paroles une fois par piste et suivre sa propre horloge audio ; il doit effacer ses données mises en cache pour le compte à la déconnexion et les actualiser lorsque la source ou le catalogue change.

## Audio authentifié

`GET /api/me/v1/playback` passe en WebSocket avec une session utilisateur ou administrateur. Le [contrat PCM versionné](server/playback.md) définit messages de démarrage/format/audio/fin, confirmations et régulation du flux, réglages DSP et historique par occurrence, sauvegardé ensemble à la fin du flux. L’extension facultative de file finie conserve le traitement entre transitions compatibles avec des confirmations cumulatives. L’extension interactive distincte ajoute déplacement dans la source, édition de la suite avec contrôle de révision et générations/frontières de remise à zéro confirmées. Un profil privé facultatif conserve file/réglages/position confirmée pour une reprise explicite après reconnexion du compte ; il ne relance pas automatiquement l’audio et ne duplique pas les écoutes précédentes. Il emploie le propriétaire de la session et accepte seulement les pistes actuelles du catalogue. Les deux flux de notifications restent inchangés. Ce transport natif est distinct de la diffusion de fichiers originaux Subsonic/OpenSubsonic.

La lecture native décodée vérifie un MD5 FLAC STREAMINFO non nul à la fin du décodage complet, avant DSP. Une différence renvoie le code existant `decode_failed`, arrête la file et empêche une écoute complète de cette occurrence ; la consommation partielle confirmée peut encore être enregistrée. Une fermeture anticipée du client ne permet pas d’établir l’empreinte complète. L’[adaptateur de fichiers originaux](server/subsonic.md) transmet les octets encodés inchangés : la vérification du contenu décodé appartient au décodeur du client.
