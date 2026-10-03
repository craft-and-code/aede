# Suivre les changements avec WebSocket

HTTP répond quand on le demande. Un WebSocket conserve une connexion ouverte pour que le serveur annonce les changements. Ces flux transmettent des notifications, pas de musique, de catalogue complet ni de commandes. Leur frontière Host/Origin selon le transport est celle de [HTTP](http.md).

## GET /api/v1/events — ouverture WebSocket

Connectez un client WebSocket à `ws://127.0.0.1:8787/api/v1/events`. Une barre d’adresse de navigateur ou un GET curl ordinaire n’effectue pas la négociation requise. Dans la console JavaScript d’une page servie depuis la même origine locale :

```js
const events = new WebSocket('ws://127.0.0.1:8787/api/v1/events');
events.addEventListener('message', event => console.log(JSON.parse(event.data)));
```

Une page d’un autre site est refusée ; ce code ne permet pas au site public d’Aède de contrôler un serveur localhost. Un client WebSocket natif peut omettre Origin.

Le premier message est `{"type":"snapshot","scanned_at":1234567890}`. Un remplacement réussi ou retrait du catalogue produit `{"type":"catalog_changed","scanned_at":1234567890}`. Cette date est illustrative ; les deux messages peuvent avoir `scanned_at:null` sans catalogue chargé. Après un changement, relisez les pages HTTP : la notification n’est pas une mise à jour partielle du graphe. Ce flux historique contient **seulement** ces messages de catalogue, sans types de tâches ajoutés.

## GET /api/v1/activity — ouverture WebSocket

Connectez-vous à `ws://127.0.0.1:8787/api/v1/activity` pour suivre aussi les tâches. Le flux commence avec le même snapshot, annonce les changements du catalogue, puis ajoute :

| `type` | Champs en plus du type | Sens |
| --- | --- | --- |
| `task_started` | `task_id,task_kind` | Une tâche acceptée commence ; elle peut attendre le verrou. |
| `task_progress` | `task_id,task_kind,phase,done,total` | Avancement ou phase de traitement. |
| `task_completed` | `task_id,task_kind,scanned_at` | Tâche terminée ; une actualisation du catalogue a été tentée avant notification. |
| `task_failed` | `task_id,task_kind,code,message` | Échec ou annulation ; aucun completed ne suit. |
| `error` | `operation,code,message` | Échec d’un traitement sans ID, actuellement rechargement du catalogue. |

`task_kind` vaut `scan`, `identification` pour fetch ou `command` pour d’autres mutations déléguées. Les ID sont uniques dans ce processus, pas des identifiants persistants du catalogue. Ignorez types, familles, phases et champs inconnus pour garder la compatibilité.

```json
{"type":"task_progress","task_id":1,"task_kind":"scan","phase":"running","done":0,"total":0}
```

Pour les tâches HTTP asynchrones et CLI déléguées, `running` et `refreshing` indiquent un **total inconnu** (`done:0,total:0`) : n’affichez pas 0 % ou terminé. `refreshing` correspond à la publication du catalogue résultant. L’ancien scan HTTP synchrone sans corps annonce `discovered` (deux compteurs égaux aux fichiers trouvés), puis `reading` pour les fichiers nécessitant de nouveaux tags. La lecture publie environ quatre mises à jour par seconde et sa valeur finale. Le détail de fetch reste dans la CLI ou le [résultat HTTP authentifié](jobs.md).

Les codes d’échec incluent `scan_failed`, `store_error`, `catalog_unavailable`, `command_failed`, `task_cancelled` et `catalog_reload_failed`. Les messages expliquent sans constituer des clés stables. Une requête rejetée ne commence aucune tâche. `task_completed` peut précéder un `catalog_changed` ultérieur si un autre verrou empêchait la publication immédiate.

## Reconnexion et limites

Les notifications ne sont pas garanties, sans rejeu ni certitude de recevoir chaque changement intermédiaire. Après déconnexion, reconnectez-vous avec un délai croissant en cas d’échecs répétés. Une nouvelle connexion donne un état actuel du catalogue, pas l’historique des tâches. Interrogez une tâche HTTP connue et authentifiée pour retrouver état/résultat. Perdre la connexion n’annule pas le travail.

Les deux flux partagent une limite de 64 connexions ; au-delà, l’ouverture produit `503 connection_limit`. Trames/messages entrants : 1 Kio maximum. Un texte ou binaire applicatif ferme la connexion : aucun flux n’accepte de commandes. Ping/pong/close standards restent acceptés. Un envoi qui ne finit pas en cinq secondes ferme le client. Fermez les connexions inutiles. Les échecs de négociation peuvent ne pas suivre l’enveloppe JSON ; un HEAD ordinaire ne remplace pas l’ouverture WebSocket.

Détails du protocole : [contrat des événements](../../api.md#activity-stream).
