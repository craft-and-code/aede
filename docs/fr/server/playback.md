# Lecture PCM authentifiée v1

`GET /api/me/v1/playback` passe en WebSocket pour une piste actuelle du catalogue ou une file finie dont l’ordre est explicite. Fournissez exactement un en-tête `Authorization: Bearer <session>`, en WS sur HTTP local ou WSS sur [HTTPS](remote.md). Seuls `user` et `admin` peuvent lire, car la lecture écrit l’historique privé ; un `auditor` reçoit `403 forbidden`. Le jeton administratif historique n’authentifie pas cette route, même localement. Aucun paramètre d’URL n’est accepté.

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
| `sample_rate` | Entier 8000–192000 Hz. Omettre utilise la première fréquence décodée, plafonnée à 192000 Hz. La fréquence choisie reste fixe pour toute la file. |
| `bass`, `treble` | Nombres finis de −12 à +12 dB ; zéro par défaut, avec bypass à plat. |

Le serveur ouvre le fichier ordinaire du catalogue actuel et vérifie taille/date précise autour du décodage et avant publication de l’historique. Source modifiée et ancien catalogue sans date précise demandent un nouveau scan. Liens de source et dispositions multicanaux inconnues sont refusés. Fichiers audio et tags ne sont jamais réécrits.

Le premier message texte réussi décrit le format transmis :

```json
{"type":"format","track":"track:…","encoding":"f32le","sample_rate":48000,"channels":2,"duration_ms":180000,"max_unacknowledged_frames":48000}
```

`duration_ms` est la durée du catalogue ou `null`, pas un nombre de trames garanti. La disposition décodée est mono ou stéréo ; les sources multicanaux reconnues utilisent le mélange stéréo partagé, sans LFE. Le traitement emploie `PcmSession` : conversion de fréquence facultative, gain fixe de normalisation, tonalité/marge puis garde finale des échantillons. La politique partagée de gain disponible réutilise tags et mesures de piste existantes ; ce transport ne mesure pas de nouvelle sonie. Le mode album examine le programme complet demandé pour utiliser les tags d’album disponibles ou un cache correspondant exactement à son ordre ; sinon, il conserve le niveau décodé, sans remplacer une mesure d’album absente par la sonie de piste. Pour la demande initiale d’une seule piste, le programme reste cet unique chemin. La préparation en mode album peut examiner tous les fichiers demandés avant le premier message de format. Le traitement ne garantit ni bit-perfect ni plafond physique de crête vraie. Le client devrait afficher les réglages choisis plutôt que présenter le PCM traité comme le fichier encodé original.

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

Le serveur ferme ensuite la connexion. Une écoute vide ou inférieure à une milliseconde ferme sans message `recorded`, puisqu’elle n’enregistre aucun historique. La demande `track` conserve ce cycle d’une piste. Pour enchaîner sans reconnexion, utilisez l’extension de file finie ci-dessous. Déplacement dans une piste, modification de file pendant la lecture, commandes de répétition/mélange et persistance restent indisponibles dans ce transport.

## File finie et enchaînement continu

Envoyez `tracks` à la place de `track` pour utiliser cette extension :

```json
{"type":"start","tracks":["track:…","track:…"],"normalize":"off","sample_rate":48000,"bass":0,"treble":0}
```

Il faut exactement l’un des deux champs `track` et `tracks`. `tracks` contient de 1 à 64 références actuelles du catalogue, dans l’ordre de lecture ; le message entier reste limité à 4 Kio. Les références sont résolues avant la production ; chemins arbitraires et références indisponibles sont refusés. Une référence répétée représente une occurrence volontaire distincte. Les réglages concernent toute la file ; le serveur ne la réordonne pas et ne la conserve pas après la session.

Le premier `format` contient en plus `index: 0` et `start_frame: 0`. Le mode file transmet ces repères pour chaque occurrence, numérotée à partir de zéro :

```json
{"type":"track","index":0,"track":"track:…","start_frame":0,"duration_ms":180000}
{"type":"track_end","index":0,"track":"track:…","end_frame":8640000}
```

Les frontières décrivent la chronologie cumulative du PCM traité, y compris les trames retardées du convertisseur. `track` précède le premier PCM de l’occurrence ; `track_end` suit son dernier PCM. La durée du catalogue reste indicative. Les blocs binaires restent alignés sur des trames complètes et les confirmations ne repartent jamais de zéro entre les morceaux. Un repère annonce une position dans le flux en attente, pas que la sortie audio l’a déjà atteinte. Affichez la piste active à partir de ces frontières et de la consommation réelle.

Les transitions naturelles compatibles conservent le même `PcmSession`, convertisseur de fréquence et filtres de tonalité ; le convertisseur n’est vidé qu’à la fin du groupe compatible. Aucun silence n’est ajouté et aucune confirmation aller-retour n’est attendue à ces transitions. Un changement de format source termine l’ancien groupe et démarre un nouveau traitement ; la continuité des échantillons n’est alors pas garantie. La fréquence de sortie reste fixe. Si la sortie passe de mono à stéréo ou inversement, le serveur attend la confirmation des dernières trames de l’ancien format, puis transmet un nouveau `format` avec `index` et `start_frame` cumulatif. Consommez l’ancien format avant de reconfigurer la sortie, en conservant le compteur global de confirmation.

Il n’y a qu’un `eof` final pour toute la file. Après l’avoir entièrement consommé et confirmé, attendez un `recorded` pour chaque occurrence sauvegardée ; ces confirmations comportent en plus `index` et `track`. Une occurrence de moins d’une milliseconde ne produit pas d’historique. Gardez la sortie audio du client ouverte entre pistes compatibles et prévoyez un tampon borné avec une marge suffisante. Un PCM serveur continu ne garantit pas l’absence de coupure dans un périphérique client ou sur une machine surchargée.

## Historique d’écoute

La demande d’une seule piste enregistre au plus un `Play` ; une file enregistre au plus un événement par occurrence, avec la règle personnelle existante et le propriétaire authentifié. `ms_played` correspond aux trames confirmées dans les frontières de cette occurrence, divisées par leur fréquence et arrondies à la milliseconde inférieure. Moins d’une milliseconde n’enregistre rien. `completed` exige une fin de décodage valide pour l’occurrence et la confirmation de toutes ses trames. L’envoi seul ne compte jamais une écoute complète. Une fermeture pendant la seconde piste peut ainsi conserver une première écoute complète et une seconde partielle, sans compter les morceaux suivants.

Les événements de la file restent en mémoire bornée puis sont sauvegardés ensemble à la fin ou à la déconnexion, hors décodage et production réseau. Ils ne sont pas sauvegardés durablement à chaque transition : un arrêt brutal du processus avant publication peut perdre ces écoutes en attente. Chaque source est revérifiée séparément sous le verrou partagé des données ; une occurrence invalidée ne peut pas être sauvegardée, tandis que les précédentes encore valides peuvent l’être, avec une erreur d’historique signalée. Révocation ou perte des permissions empêchent la publication tardive. Une confirmation finale perdue ne permet pas de savoir quelles écritures ont échoué.

Fermeture anticipée normale, délai dépassé ou échec de traitement peuvent enregistrer l’audio partiel confirmé comme une écoute incomplète, si source et session permettent encore l’écriture. Révocation ou identité de source modifiée/indisponible empêchent une écriture tardive. Les confirmations déclarent la consommation du client, sans prouver qu’une personne a entendu le son. Ne soumettez pas aussi cette écoute à la route explicite d’historique : elle enregistre des événements indépendants et ne déduplique pas un rapport client séparé. Perdre la confirmation finale ne prouve pas l’échec de l’écriture ; ne répétez pas aveuglément une soumission d’historique.

## Capacités et erreurs

Quatre places bornent décodages actifs et travail d’historique associé. Les passages supplémentaires en WebSocket renvoient `503 playback_busy` ; session invalide : `401 unauthorized`, auditeur : `403 forbidden`. Les commandes sont limitées à 4 Kio et les files finies à 64 occurrences. Le producteur possède quatre événements en attente et la fenêtre client vaut une seconde. Démarrage/préparation de source, absence de progrès de consommation et production de source bloquée ont un délai de dix secondes ; chaque envoi possède cinq secondes. Un client en pause doit reprendre avant le délai de consommation ou se reconnecter. La confirmation de publication de l’historique attend un temps borné ; un verrou de données occupé est réessayé pendant au plus cinq secondes avant échec.

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
