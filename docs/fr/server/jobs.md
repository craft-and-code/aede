# Scanner, enrichir, suivre et annuler les tâches

Chaque requête utilise [l’authentification d’administration](administration.md). Les exemples supposent que `AEDE_ADMIN_TOKEN` contient le secret du serveur actif. Les chemins du corps désignent **l’ordinateur serveur**. Les routes de tâches refusent les paramètres d’URL.

## POST /api/admin/v1/scan — sans corps

Le mode de compatibilité rescane les racines suivies de manière synchrone, en conservant exclusions, fusions d’artistes et conclusions indépendantes. La connexion attend la sauvegarde et la publication.

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/scan' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Renvoie HTTP 200 avec `{status:"completed",scanned_at,files}`. La date est en secondes Unix et files compte les fichiers du catalogue découvert. Ce mode n’ajoute pas de dossiers par un corps et n’est pas une tâche HTTP asynchrone consultable. Un autre rédacteur donne `409 store_busy` ; échec de scan/stockage : `500 scan_failed` ou `store_error`. Envoyez `{}` pour choisir le mode asynchrone.

## POST /api/admin/v1/scan — objet JSON

Un objet, même vide, crée une tâche de scan asynchrone :

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/scan' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"folders":["/chemin/vers/musique"],"full":false,"threads":0}'
```

| Champ | Type/défaut | Sens |
| --- | --- | --- |
| `folders` | tableau de textes, vide | Ajoute des racines absolues sur le serveur ; vide rescane celles déjà suivies. Maximum 64 chemins, 4096 octets chacun, sans NUL. |
| `replace` | booléen, false | Remplace les racines suivies au lieu d’ajouter ; exige un dossier au moins. Change explicitement la sélection, sans supprimer l’audio. |
| `full` | booléen, false | Relit les tags même si un scan incrémental pouvait réutiliser les données ; conserve exclusions/conclusions. |
| `threads` | entier, automatique si absent | 0 choisit automatiquement ; plage acceptée 0–64. |
| `follow_symlinks` | booléen, false | Suit les liens symboliques pendant la découverte. |
| `include_hidden` | booléen, false | Inclut les éléments cachés. |

Renvoie HTTP 202, par exemple `{"task_id":1,"status":"queued","status_url":"/api/admin/v1/tasks/1"}`. **202 signifie accepté, pas terminé**. Conservez ID/adresse et suivez la tâche. Chemin/combinaison invalide : `400 invalid_parameters` ; JSON/champ inconnu : `400 invalid_body` ; capacité : `503 task_limit`.

## POST /api/admin/v1/fetch

Exige un objet JSON. Fetch est l’étape en ligne explicitement demandée : il obtient des informations attribuées sans remplacer les tags locaux.

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/fetch' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"targets":["Miles Davis"],"summaries":true,"lang":"fr","dry_run":true}'
```

Cet exemple prévisualise le travail sans contacter les services externes. Retirez `dry_run:true` lorsque vous souhaitez l’effectuer. Même réponse HTTP 202 de tâche en attente que pour scan ; le statut ultérieur aura `task_kind:"identification"`.

Tous les booléens valent false par défaut ; targets est vide. **`{}` n’est pas une absence d’action** : sans choix de passe, il lance l’identification MusicBrainz habituelle de la CLI.

| Champ(s) | Sens / contraintes |
| --- | --- |
| `targets` | Au plus 64 noms d’artistes/albums ou chemins de dossiers serveur non vides, 4096 octets chacun, sans NUL. Un chemin absolu évite l’ambiguïté de dossier. |
| `summaries` | Télécharge les textes d’artistes/albums via les sources de la CLI. |
| `discography` | Télécharge la discographie pour examiner ensuite les albums manquants. |
| `covers` | Obtient les informations de pochettes ; leur téléchargement exige `images`. |
| `lyrics` | Recherche explicitement les paroles, jamais activées implicitement. |
| `identify` | Active la passe d’identification CLI. |
| `credits` | Obtient les crédits d’albums/interprétations/œuvres avec leur périmètre existant. |
| `recordings` | Obtient l’identification/information des enregistrements. |
| `portraits` | Obtient les portraits d’artistes pris en charge. |
| `logos` | Obtient les logos d’artistes/labels pris en charge. |
| `labels` | Obtient les informations de labels. |
| `fanart` | Active les familles d’illustrations Fanart.tv. |
| `banners` | Inclut les bannières ; exige `logos` ou `fanart`. |
| `images` | Télécharge les pochettes ; exige `covers`. |
| `size` | Texte `250`, `500`, `1200`, `original` ; `full` est un alias d’original. Exige `covers`. |
| `lang` | Langues de textes selon la CLI ; texte non vide, au plus 256 octets/sans NUL. |
| `full` | Répète les recherches déjà enregistrées. |
| `dry_run` | Prévisualise sans requête externe. |
| `yes` | Accepte explicitement la confirmation d’un grand traitement ; HTTP n’attend pas de réponse interactive. |
| `no_logo,no_label_logo,no_portrait,no_background,no_banner,no_album_cover,no_cdart` | Exclusions booléennes de familles d’images ; toutes exigent `fanart`. Ne combinez pas `logos`/`no_logo` ou `banners`/`no_banner`. |

Dépendances et choix suivent [le guide des sources](../../sources.md). Les clés des services viennent du serveur ; le client ne remplace ni clés, ni dossier de données, ni arguments d’exécutable. Limites de débit, sauvegarde de progression et absence d’écrasement restent actives. Images/paroles peuvent créer des fichiers dérivés à côté de la musique ; audio/tags restent intacts. Sans `yes`, une tâche non interactive nécessitant confirmation échoue au lieu d’attendre. Options/plages invalides : 400 ; une tâche acceptée peut ensuite échouer dans son résultat.

## GET /api/admin/v1/tasks/{id}

Utilisez l’ID numérique du scan/fetch soumis en JSON, pas une référence du catalogue ni un ID CLI délégué.

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/tasks/1' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Renvoie HTTP 200 avec :

```json
{"task_id":1,"task_kind":"scan","status":"completed","cancel_requested":false,"result":{"exit_code":0,"stdout":"...","stderr":"","output_truncated":false},"error":null}
```

Statuts : `queued,running,completed,failed,cancelled`. Result est null jusqu’à disponibilité ; stdout et stderr sont limités chacun à 64 Kio. `output_truncated:true` indique un affichage incomplet. Un échec contient `error:{code,message}` ; un code de sortie 0 indique la réussite. Interrogez avec un délai raisonnable ; [activity](events.md) annonce les phases mais ne conserve pas durablement les résultats. ID inconnu/expiré : `404 task_not_found` ; il doit être un entier décimal strictement positif (sinon `400 invalid_task_id`). Un échec imprévu du registre donne `500 task_error`. Les tâches déléguées et le scan de compatibilité sans corps ne sont pas sélectionnables ici.

### HEAD /api/admin/v1/tasks/{id}

Même authentification/validation, statut/en-têtes sans résultat. `curl -I URL -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"` vérifie l’existence.

## POST /api/admin/v1/tasks/{id}/cancel

Sans corps ni paramètres d’URL.

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/tasks/1/cancel' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Renvoie HTTP 202 `{task_id,status:"cancel_requested",status_url}`. C’est une demande d’arrêt à un point sûr, pas la preuve d’un arrêt déjà effectué. Suivez jusqu’au statut final. La progression enregistrée **n’est pas annulée**. Tâche finie : `409 task_not_cancellable` ; ID inconnu/expiré : 404.

## Durée de vie et nouvelle tentative

Maximum quatre tâches HTTP actives et 64 dossiers conservés ; les plus anciennes tâches terminées sont évincées. Les tâches attendent le verrou partagé et survivent à la déconnexion du client. L’arrêt propre du serveur les attend : annulez d’abord un long fetch à interrompre. ID/résultats vivent seulement en mémoire et disparaissent au redémarrage.

Un POST dont la réponse a été perdue peut déjà avoir démarré. Les soumissions ne sont pas idempotentes : ne les répétez pas aveuglément. Reconnectez-vous/suivez un ID conservé et examinez activité/journaux avant une nouvelle soumission. La récupération ne rembobine ni requêtes externes ni résultats partiels sauvegardés.
