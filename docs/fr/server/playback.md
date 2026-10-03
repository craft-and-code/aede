# Lecture PCM authentifiée v1

`GET /api/me/v1/playback` passe en WebSocket pour une piste actuelle du catalogue. Fournissez exactement un en-tête `Authorization: Bearer <session>`, en WS sur HTTP local ou WSS sur [HTTPS](remote.md). Seuls `user` et `admin` peuvent lire, car la lecture écrit l’historique privé ; un `auditor` reçoit `403 forbidden`. Le jeton administratif historique n’authentifie pas cette route, même localement. Aucun paramètre d’URL n’est accepté.

Ce contrat vise les clients natifs. Le client possède sa sortie audio, son horloge et son compteur de consommation. Aède n’ouvre pas de périphérique audio sur le serveur, ne fournit pas d’interface de lecteur, n’accepte pas de chemin arbitraire et n’implémente pas Subsonic ici. Un WebSocket de navigateur ne permet pas de définir directement cet en-tête Bearer ; connexion et transport de navigateur restent à concevoir.

## Démarrage et format

Envoyez un objet JSON texte dans les dix secondes suivant le passage en WebSocket :

```json
{"type":"start","track":"track:…","normalize":"track","bass":0,"treble":0}
```

Copiez `track` depuis la référence stable d’une réponse du catalogue. Champs inconnus, champs répétés, autres types de messages et commandes binaires sont refusés. Réglages facultatifs :

| Champ | Valeurs et défaut |
| --- | --- |
| `normalize` | `off`, `track` ou `album` ; défaut `track`. |
| `sample_rate` | Entier 8000–192000 Hz. Omettre conserve la fréquence décodée, plafonnée à 192000 Hz. |
| `bass`, `treble` | Nombres finis de −12 à +12 dB ; zéro par défaut, avec bypass à plat. |

Le serveur ouvre le fichier ordinaire du catalogue actuel et vérifie taille/date précise autour du décodage et avant publication de l’historique. Source modifiée et ancien catalogue sans date précise demandent un nouveau scan. Liens de source et dispositions multicanaux inconnues sont refusés. Fichiers audio et tags ne sont jamais réécrits.

Le premier message texte réussi décrit le format transmis :

```json
{"type":"format","track":"track:…","encoding":"f32le","sample_rate":48000,"channels":2,"duration_ms":180000,"max_unacknowledged_frames":48000}
```

`duration_ms` est la durée du catalogue ou `null`, pas un nombre de trames garanti. La disposition décodée est mono ou stéréo ; les sources multicanaux reconnues utilisent le mélange stéréo partagé, sans LFE. Le traitement emploie `PcmSession` : conversion de fréquence facultative, gain fixe de normalisation, tonalité/marge puis garde finale des échantillons. La politique partagée de gain disponible réutilise tags et mesures de piste existantes ; ce socket d’une piste ne mesure pas de nouvelle sonie et ne charge pas le programme complet d’un album. Le mode album emploie les tags d’album disponibles ou un cache de programme correspondant exactement à cet unique chemin ; sinon, il conserve le niveau décodé, sans remplacer une mesure d’album absente par la sonie de piste. Le traitement ne garantit ni bit-perfect ni plafond physique de crête vraie. Le client devrait afficher les réglages choisis plutôt que présenter le PCM traité comme le fichier encodé original.

## Audio et confirmations

Chaque message binaire contient des trames complètes entrelacées en `f32` IEEE 754 petit-boutiste, au plus 32 Kio. Une trame contient un échantillon par canal, soit `channels × 4` octets. Conservez ordre et alignement des canaux. Les frontières de messages n’ont aucun sens musical.

Le client confirme cumulativement les trames seulement après leur consommation par sa sortie :

```json
{"type":"ack","frames":4096}
```

`frames` est un entier positif ou nul, jamais décroissant et jamais supérieur aux trames déjà reçues. Répéter une confirmation ne compte pas une seconde écoute et ne maintient pas un flux bloqué. Aède autorise au plus une seconde de PCM envoyé mais non confirmé, indiquée par `max_unacknowledged_frames` ; la production attend lorsque cette fenêtre est pleine. Une confirmation valide croissante compte comme activité authentifiée du client ; envois serveur et ping/pong ne prolongent pas l’inactivité de session. Expiration absolue et révocation restent applicables.

À la fin du décodage, le serveur envoie :

```json
{"type":"eof","frames":8640000}
```

Le total inclut la fin du convertisseur. Consommez/confirmez toutes les trames restantes, puis attendez la confirmation de sauvegarde de l’historique :

```json
{"type":"recorded","ms_played":180000,"completed":true}
```

Le serveur ferme ensuite la connexion. Une écoute vide ou inférieure à une milliseconde ferme sans message `recorded`, puisqu’elle n’enregistre aucun historique. Reconnectez-vous avec un nouveau start pour une autre piste. File d’attente, déplacement dans une piste et jonctions distantes continues ne font pas partie de v1.

## Historique d’écoute

Un socket enregistre au plus un `Play` avec la règle personnelle existante et le propriétaire authentifié. `ms_played` correspond aux trames de sortie confirmées divisées par leur fréquence, arrondies à la milliseconde inférieure. Moins d’une milliseconde n’enregistre rien. `completed` exige une fin de décodage valide et la confirmation de toutes les trames envoyées. L’envoi seul ne compte jamais une écoute complète.

Fermeture anticipée normale, délai dépassé ou échec de traitement peuvent enregistrer l’audio partiel confirmé comme une écoute incomplète, si source et session permettent encore l’écriture. Révocation ou identité de source modifiée/indisponible empêchent une écriture tardive. Les confirmations déclarent la consommation du client, sans prouver qu’une personne a entendu le son. Ne soumettez pas aussi cette écoute à la route explicite d’historique : elle enregistre des événements indépendants et ne déduplique pas un rapport client séparé. Perdre la confirmation finale ne prouve pas l’échec de l’écriture ; ne répétez pas aveuglément une soumission d’historique.

## Capacités et erreurs

Quatre places bornent décodages actifs et travail d’historique associé. Les passages supplémentaires en WebSocket renvoient `503 playback_busy` ; session invalide : `401 unauthorized`, auditeur : `403 forbidden`. Les commandes sont limitées à 4 Kio. Le producteur possède quatre blocs audio en attente et la fenêtre client vaut une seconde. Démarrage/préparation de source, absence de progrès de consommation et production de source bloquée ont un délai de dix secondes ; chaque envoi possède cinq secondes. Un client en pause doit reprendre avant le délai de consommation ou se reconnecter. La confirmation de publication de l’historique attend un temps borné ; un verrou de données occupé est réessayé pendant au plus cinq secondes avant échec.

Après passage en WebSocket, un échec envoie `{"type":"error","code":"…","message":"…"}` si la connexion permet encore d’écrire, puis ferme. Examinez `code` ; le texte humain peut changer :

| Code | Sens |
| --- | --- |
| `invalid_start`, `invalid_ack` | Commande, réglage, type de référence ou compteur invalides. |
| `track_not_found`, `catalog_unavailable` | Actualiser catalogue/référence avant de réessayer. |
| `source_changed`, `source_unavailable` | Restaurer ou rescanner la source audio ordinaire. |
| `decode_failed`, `processing_failed`, `stream_failed` | Échec du décodeur, DSP ou travailleur ; conserver le sens d’écoute partielle. |
| `ack_timeout` | Aucun progrès de consommation client à temps. |
| `authentication_expired` | Session expirée/révoquée ou compte ne permettant plus la lecture. |
| `history_failed` | Sauvegarde de l’historique non confirmée. |
| `server_shutdown` | Arrêt du serveur. |

Une panne de transport ou un client fermé ne garantit ni erreur JSON ni confirmation finale. Les restrictions communes des [comptes](accounts.md) et de [HTTPS](remote.md) concernent négociation et flux actif. Les contrôles automatisés PCM/transport ne valident ni gapless physique, qualité du périphérique client ni capacité du NAS cible.
