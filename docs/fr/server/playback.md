# Lecture PCM authentifiée v1

`GET /api/me/v1/playback` passe en WebSocket pour une piste actuelle du catalogue ou une file finie dont l’ordre est explicite. Fournissez exactement un en-tête `Authorization: Bearer <session>`, en WS sur HTTP local ou WSS sur [HTTPS](remote.md). Seuls `user` et `admin` peuvent lire, car la lecture écrit l’historique privé ; un `auditor` reçoit `403 forbidden`. Le jeton administratif historique n’authentifie pas cette route, même localement. Aucun paramètre d’URL n’est accepté.

Ce contrat vise les clients natifs. Le client possède sa sortie audio, son horloge et son compteur de consommation. Aède n’ouvre pas de périphérique audio sur le serveur, ne fournit pas d’interface de lecteur, n’accepte pas de chemin arbitraire et n’implémente pas Subsonic ici. Un WebSocket de navigateur ne permet pas de définir directement cet en-tête Bearer ; connexion et transport de navigateur restent à concevoir.

## Démarrage et format

Envoyez un objet JSON texte dans les dix secondes suivant le passage en WebSocket :

```json
{"type":"start","track":"track:…","normalize":"track","bass":0,"treble":0}
```

Copiez `track` depuis la référence stable d’une réponse du catalogue. Champs inconnus, champs répétés, types de messages non documentés et commandes binaires sont refusés. L’extension interactive ci-dessous définit aussi un `resume` initial et des commandes pendant la lecture. Réglages facultatifs :

| Champ | Valeurs et défaut |
| --- | --- |
| `normalize` | `off`, `track` ou `album` ; défaut `track`. |
| `sample_rate` | Entier 8000–192000 Hz. Omettre utilise la première fréquence décodée, plafonnée à 192000 Hz. La fréquence choisie reste fixe pour toute la file. |
| `bass`, `treble` | Nombres finis de −12 à +12 dB ; zéro par défaut, avec bypass à plat. |

Le serveur ouvre le fichier ordinaire du catalogue actuel et vérifie taille/date précise autour du décodage et avant publication de l’historique. Source modifiée et ancien catalogue sans date précise demandent un nouveau scan. Les chemins de source du catalogue doivent être absolus et sans remontée vers un dossier parent (`..`) ; les chemins mal formés sont refusés plutôt que résolus depuis le dossier de travail du serveur. Liens de source et dispositions multicanaux inconnues sont refusés. Fichiers audio et tags ne sont jamais réécrits.

Pour un FLAC dont STREAMINFO contient un MD5 non nul, atteindre la fin décodée vérifie aussi le PCM entier complet contre cette empreinte, avant conversion en flottants, normalisation, mélange de canaux ou autre DSP. Une différence arrête la lecture avec `decode_failed`, sans `eof` réussi ni `track_end` pour l’occurrence en échec. Du PCM peut déjà avoir atteint le client : les confirmations acceptées peuvent encore produire une écoute incomplète. Les occurrences précédentes terminées et entièrement confirmées sont conservées ; les pistes suivantes ne démarrent pas. Un FLAC sans empreinte reste lisible, mais aucune vérification MD5 n’est annoncée. Fermer avant la fin décodée ne permet pas d’établir l’empreinte complète. Ce contrôle concerne le contenu décodé de la source encodée, pas son identité ni la sortie physique du client, et ne remplace pas les vérifications d’identité de source.

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

Le serveur ferme ensuite la connexion. Une écoute vide ou inférieure à une milliseconde ferme sans message `recorded`, puisqu’elle n’enregistre aucun historique. La demande `track` conserve ce cycle d’une piste. Pour enchaîner sans reconnexion, utilisez l’extension de file finie ci-dessous. Déplacement, édition de la suite future et reprise persistante demandent l’extension interactive facultative distincte ; les commandes serveur de répétition/aléatoire restent indisponibles.

## File finie et enchaînement continu

Envoyez `tracks` à la place de `track` pour utiliser cette extension :

```json
{"type":"start","tracks":["track:…","track:…"],"normalize":"off","sample_rate":48000,"bass":0,"treble":0}
```

Il faut exactement l’un des deux champs `track` et `tracks`. `tracks` contient de 1 à 64 références actuelles du catalogue, dans l’ordre de lecture ; le message entier reste limité à 4 Kio. Les références sont résolues avant la production ; chemins arbitraires et références indisponibles sont refusés. Une référence répétée représente une occurrence volontaire distincte. Les réglages concernent toute la file ; ce mode fini historique ne la réordonne pas et ne la conserve pas après la session.

Le premier `format` contient en plus `index: 0` et `start_frame: 0`. Le mode file transmet ces repères pour chaque occurrence, numérotée à partir de zéro :

```json
{"type":"track","index":0,"track":"track:…","start_frame":0,"duration_ms":180000}
{"type":"track_end","index":0,"track":"track:…","end_frame":8640000}
```

Les frontières décrivent la chronologie cumulative du PCM traité, y compris les trames retardées du convertisseur. `track` précède le premier PCM de l’occurrence ; `track_end` suit son dernier PCM. La durée du catalogue reste indicative. Les blocs binaires restent alignés sur des trames complètes et les confirmations ne repartent jamais de zéro entre les morceaux. Un repère annonce une position dans le flux en attente, pas que la sortie audio l’a déjà atteinte. Affichez la piste active à partir de ces frontières et de la consommation réelle.

Les transitions naturelles compatibles conservent le même `PcmSession`, convertisseur de fréquence et filtres de tonalité ; le convertisseur n’est vidé qu’à la fin du groupe compatible. Aucun silence n’est ajouté et aucune confirmation aller-retour n’est attendue à ces transitions. Un changement de format source termine l’ancien groupe et démarre un nouveau traitement ; la continuité des échantillons n’est alors pas garantie. La fréquence de sortie reste fixe. Si la sortie passe de mono à stéréo ou inversement, le serveur attend la confirmation des dernières trames de l’ancien format, puis transmet un nouveau `format` avec `index` et `start_frame` cumulatif. Consommez l’ancien format avant de reconfigurer la sortie, en conservant le compteur global de confirmation.

Il n’y a qu’un `eof` final pour toute la file. Après l’avoir entièrement consommé et confirmé, attendez un `recorded` pour chaque occurrence sauvegardée ; ces confirmations comportent en plus `index` et `track`. Une occurrence de moins d’une milliseconde ne produit pas d’historique. Gardez la sortie audio du client ouverte entre pistes compatibles et prévoyez un tampon borné avec une marge suffisante. Un PCM serveur continu ne garantit pas l’absence de coupure dans un périphérique client ou sur une machine surchargée.

## Files interactives, déplacement et reprise persistante

L’extension interactive est facultative. Les demandes historiques d’une seule piste ou d’une file finie gardent leurs messages et compteurs cumulatifs. Pour activer les commandes, ajoutez `interactive: true` à `start` ; ce mode utilise les règles de file même avec un seul `track` :

```json
{"type":"start","tracks":["track:…","track:…"],"interactive":true,"profile":"desktop","normalize":"off"}
```

`profile` est facultatif et active un point de reprise privé persistant. Il contient 1 à 64 lettres ASCII, chiffres, tirets bas ou tirets. Sans lui, la file interactive reste temporaire. `profile` est accepté uniquement en mode interactif. Un seul socket actif peut posséder un même compte/profil ; un autre reçoit `state_conflict`, tandis que les profils distincts restent indépendants. Un profil appartient au compte authentifié ; ce n’est ni un nom de compte ni une playlist partagée. Une référence de piste peut se répéter, avec un identifiant stable d’occurrence distinct pour chaque entrée de file.

Les messages interactifs indiquent une génération de sortie `epoch` et une `revision` de file. Chaque connexion commence à la génération 0 ; un nouveau profil commence à la révision 1, tandis qu’un profil réutilisé/repris augmente sa révision sauvegardée. Le message initial `queue` donne la file, la révision, l’occurrence actuelle et la position source de référence :

```json
{"type":"queue","epoch":0,"revision":1,"items":[{"occurrence":1,"track":"track:…"},{"occurrence":2,"track":"track:…"}],"current_occurrence":1,"position_ms":0}
```

Conservez ces identifiants d’occurrence, sans les déduire de l’index de file ou de la référence de piste. Les messages interactifs `format`, `track`, `track_end` et `eof` portent aussi `epoch`/`revision` ; les messages liés à une occurrence portent en plus `occurrence`. `format` et `track` portent `position_ms`, le décalage source du nouveau segment. Les confirmations incluent la génération actuelle ; leur compteur de trames est cumulatif **dans cette génération** :

```json
{"type":"ack","epoch":0,"frames":4096}
```

Une commande porte les trames réellement consommées au moment de son émission. Consommation et changement demandé forment ainsi une seule opération ; n’incluez jamais les trames en attente qui vont être abandonnées. Exemples :

```json
{"type":"seek","epoch":0,"frames":4096,"position_ms":30000}
{"type":"edit_queue","epoch":0,"frames":4096,"revision":1,"items":[{"occurrence":2},{"track":"track:…"}]}
{"type":"stop","epoch":0,"frames":4096}
```

Le déplacement choisit une position absolue positive ou nulle en millisecondes dans l’occurrence actuellement consommée, au plus 24 heures. Une demande au-delà de la fin réellement décodée est refusée avec `invalid_seek` ; la fin exacte fait avancer naturellement vers l’occurrence suivante. Le décodage progressif reste soumis au délai de production : un début très long peut donc produire un refus explicite plutôt que monopoliser un travailleur. Il décode progressivement le début de la source puis relance le traitement à la position demandée ; ce n’est pas une plage d’octets. Une visite affectée par un déplacement reste une écoute incomplète même si le décodage atteint ensuite la fin. La durée écoutée cumule l’audio confirmé sur ses segments, sans compter le début sauté.

`edit_queue.items` remplace uniquement la suite future : les occurrences passées et l’occurrence actuelle sont conservées. Réutilisez une `occurrence` future pour la déplacer ; omettez-la pour la supprimer. Un `track` crée une nouvelle occurrence depuis une référence actuelle du catalogue. Un identifiant existant doit appartenir à la suite future et ne peut apparaître deux fois dans son remplacement. Les nouvelles références de piste peuvent se répéter volontairement. La file résultante complète est limitée à 64 occurrences. `revision` protège contre une édition fondée sur une ancienne version de la file. Pour passer l’occurrence actuelle, déplacez-vous jusqu’à sa fin ; sa suppression par édition de la suite est refusée.

Un déplacement ou une édition réussis abandonnent le PCM préchargé, augmentent `epoch` et renvoient une frontière de remise à zéro :

```json
{"type":"reset","epoch":1,"revision":2,"items":[{"occurrence":1,"track":"track:…"},{"occurrence":2,"track":"track:…"}],"current_occurrence":1,"position_ms":30000}
```

Le client doit cesser d’utiliser les trames de l’ancienne génération, vider les buffers non consommés de l’application et du périphérique, remettre son compteur cumulatif à zéro et confirmer la frontière :

```json
{"type":"reset_ack","epoch":1}
```

Le serveur envoie ensuite seulement le nouveau format, la position de piste et le PCM. Le décalage source renvoyé fait partie de la nouvelle horloge : les trames de génération commencent à zéro, mais la piste peut commencer au milieu. Les confirmations et commandes d’une ancienne génération sont refusées. Chaque déplacement ou édition réussis augmente génération et révision. Envoyez `reset_ack` dans les dix secondes ; aucun nouveau PCM ne précède cette confirmation, et authentification/révocation/arrêt serveur restent vérifiés pendant l’attente. Modifier la suite peut donc interrompre brièvement l’audio actuel pour remplacer le préchargement. Seules les transitions naturelles compatibles inchangées conservent le traitement continu ; une commande utilisateur n’est pas annoncée comme gapless. `stop` demande la sauvegarde de l’écoute confirmée et du point de reprise, puis termine le socket.

### Reprise explicite et durabilité du point de reprise

Reconnectez-vous avec une session de compte valide, puis envoyez ceci comme premier message :

```json
{"type":"resume","profile":"desktop"}
```

La reprise restaure la file sauvegardée, l’occurrence actuelle, la position source confirmée et les réglages de lecture. Elle n’accepte ni nouvelle piste ni remplacement de réglages. Le serveur vérifie les sources sauvegardées contre le catalogue actuel, leur taille et leur date précise ; une source absente ou modifiée demande un nouveau scan et un nouveau démarrage, sans substitution silencieuse par un fichier de même nom. Un redémarrage du serveur invalide les sessions Bearer : reconnectez le compte avant la reprise. Aucun son ne démarre automatiquement au redémarrage du serveur. Une file entièrement consommée est sauvegardée avec `current_occurrence: null` et `position_ms: 0` ; sa reprise annonce la file, confirme son état terminé et ferme sans rejouer la première piste.

Une connexion reprise commence une nouvelle écoute. L’audio confirmé sur le socket précédent n’est pas recompté. Une occurrence reprise après son début reste incomplète puisque son début a été entendu dans une autre connexion ; une occurrence sauvegardée à la position zéro peut être complète si cette nouvelle connexion confirme tout son audio. Les occurrences suivantes sans déplacement peuvent se terminer normalement. Le point de reprise conserve position et réglages ; il ne prouve pas une écoute et ne crée pas un événement `Play` supplémentaire. Les positions sont arrondies à la milliseconde inférieure ; la réouverture après une édition ou une reprise peut rejouer moins d’une milliseconde d’audio source. Un déplacement refusé ou non confirmé conserve le dernier point validé.

Les confirmations croissantes programment une sauvegarde au plus toutes les cinq secondes. Le travailleur d’écriture borné tient le disque à l’écart de la production PCM ; les commandes réussies et une fin/déconnexion normale demandent une sauvegarde finale. Un verrou de données occupé peut retarder la publication. Un arrêt brutal du processus peut perdre les cinq dernières secondes plus le retard d’écriture, ainsi que les écoutes conservées en mémoire ; aucune consommation n’est inventée pour combler ce manque. Un point de reprise sauvegardé ne rend pas chaque confirmation acceptée durable. Ces points sont stockés dans les données personnelles privées de `user.json` et inclus dans les sauvegardes privées ; une réinitialisation du catalogue les conserve. La suppression du compte rend leur propriétaire indisponible. Commandes et sauvegardes finales attendent au plus dix secondes ; une acquisition individuelle du verrou occupé est réessayée pendant au plus cinq secondes. Un échec de publication est explicite.

En fin normale, les confirmations d’écoute interactives utilisent `occurrence` stable avec `track`, `ms_played` et `completed`. Après ces confirmations, attendez le résultat de sauvegarde avant d’annoncer le dernier point de reprise sauvegardé :

```json
{"type":"saved","profile":"desktop","revision":2,"completed":false}
```

`completed` indique que la fin du reste de la file a été atteinte et confirmée ; chaque message `recorded` précise séparément si sa visite était complète. Pour une file interactive temporaire, `profile` vaut `null` : cela n’annonce aucun point de reprise durable. Perdre cette confirmation finale laisse l’écriture incertaine et ne doit pas provoquer une double soumission d’historique.

Le serveur conserve au plus 16 profils par propriétaire et 512 au total, sans éviction automatique d’un autre profil. Chaque connexion interactive accepte au plus 128 commandes et conserve au plus 256 visites écoutées. Ces limites bornent mémoire, relances du décodeur et persistance. Le client doit expliquer un refus, sans répéter indéfiniment les commandes. Répétition et aléatoire restent des choix du client sur une file bornée ; cette extension n’ajoute aucune commande serveur pour ces modes.

## Paroles et horloge du client

Récupérer les paroles locales séparément avec `GET /api/v1/lyrics?track=<référence stable>` et l’authentification du catalogue par session. L’[API de paroles](../api.md#paroles-locales) fournit le texte complet, sa catégorie d’origine et les horodatages éventuels en millisecondes. Elle n’insère aucune parole dans le PCM, ne change pas le contrôle du flux audio et ne crée aucun historique d’écoute. Cette route native n’ajoute pas de méthode de paroles Subsonic/OpenSubsonic.

Récupérer les données une fois par piste actuelle, puis suivre la position de l’audio effectivement consommé par la sortie du client. Recevoir de l’audio, un marqueur `track` ou une réponse de paroles ne signifie pas que le périphérique a atteint cette position. Dans une file PCM finie, rattacher le compteur cumulatif de frames consommées à la bonne occurrence grâce à `start_frame`/`end_frame`, soustraire son début et convertir avec la fréquence transmise. Si le client rééchantillonne, ramener d’abord la consommation du périphérique aux frames transmises. Une piste répétée recommence sa position de paroles ; l’occurrence suivante peut réutiliser les mêmes données source.

Regrouper les horodatages égaux dans l’ordre source, puis trier les groupes chronologiquement. Choisir le dernier groupe dont `at_ms` ne dépasse pas la position de la piste ; ne rien afficher avant le premier groupe horodaté. Un groupe horodaté vide efface les mots précédents ; le dernier reste actif jusqu’à la fin, sauf si un repère vide l’efface. Le décalage LRC est déjà appliqué : ne pas le réappliquer. La pause fige l’horloge. Un client pouvant se déplacer avec un autre transport audio recalcule le groupe actif après chaque déplacement au lieu de rejouer les lignes intermédiaires. En PCM interactif, ajoutez le décalage source renvoyé à la position consommée dans la piste de la nouvelle génération, puis recalculez après chaque remise à zéro ; l’ancien PCM abandonné ne fait jamais avancer les paroles.

Des paroles mêlant lignes horodatées et lignes simples conservent tout le texte source : seules les lignes horodatées commandent la surbrillance, tandis que le texte complet reste lisible. `lyrics: null` signifie aucune parole locale non vide ; une source indisponible, obsolète ou excessive produit une erreur explicite. Ces règles facultatives figurent aussi dans [Compatible Aède](compatible-aede.md#paroles-synchronisées-facultatives).

## Historique d’écoute

La demande d’une seule piste enregistre au plus un `Play` ; une file enregistre au plus un événement par occurrence écoutée dans cette connexion, avec la règle personnelle existante et le propriétaire authentifié. Les remises à zéro interactives cumulent les trames confirmées avant un seul arrondi de la durée de visite, y compris pour les segments de moins d’une milliseconde ; un déplacement rend sa visite incomplète. Une connexion reprise commence une visite distincte sans rejouer l’historique de la connexion précédente ; une position source reprise non nulle garde cette visite incomplète. `ms_played` correspond aux trames confirmées dans les frontières de cette occurrence, divisées par leur fréquence et arrondies à la milliseconde inférieure. Moins d’une milliseconde n’enregistre rien. `completed` exige une fin de décodage valide pour l’occurrence et la confirmation de toutes ses trames. L’envoi seul ne compte jamais une écoute complète. Une fermeture pendant la seconde piste peut ainsi conserver une première écoute complète et une seconde partielle, sans compter les morceaux suivants.

Les événements de la file restent en mémoire bornée puis sont sauvegardés ensemble à la fin ou à la déconnexion, hors décodage et production réseau. Ils ne sont pas sauvegardés durablement à chaque transition : un arrêt brutal du processus avant publication peut perdre ces écoutes en attente. Chaque source est revérifiée séparément sous le verrou partagé des données ; une occurrence invalidée ne peut pas être sauvegardée, tandis que les précédentes encore valides peuvent l’être, avec une erreur d’historique signalée. Révocation ou perte des permissions empêchent la publication tardive. Une confirmation finale perdue ne permet pas de savoir quelles écritures ont échoué.

Fermeture anticipée normale, délai dépassé ou échec de traitement peuvent enregistrer l’audio partiel confirmé comme une écoute incomplète, si source et session permettent encore l’écriture. Révocation ou identité de source modifiée/indisponible empêchent une écriture tardive. Les confirmations déclarent la consommation du client, sans prouver qu’une personne a entendu le son. Ne soumettez pas aussi cette écoute à la route explicite d’historique : elle enregistre des événements indépendants et ne déduplique pas un rapport client séparé. Perdre la confirmation finale ne prouve pas l’échec de l’écriture ; ne répétez pas aveuglément une soumission d’historique.

## Capacités et erreurs

Quatre places bornent décodages actifs et travail d’historique associé. Les passages supplémentaires en WebSocket renvoient `503 playback_busy` ; session invalide : `401 unauthorized`, auditeur : `403 forbidden`. Les commandes sont limitées à 4 Kio et les files finies à 64 occurrences. Le producteur possède quatre événements en attente et la fenêtre client vaut une seconde. Démarrage/préparation de source, absence de progrès de consommation et production de source bloquée ont un délai de dix secondes ; chaque envoi possède cinq secondes. Un client en pause doit reprendre avant le délai de consommation ou se reconnecter. La confirmation de publication de l’historique attend un temps borné ; un verrou de données occupé est réessayé pendant au plus cinq secondes avant échec.

Après passage en WebSocket, un échec envoie `{"type":"error","code":"…","message":"…"}` si la connexion permet encore d’écrire, puis ferme. Examinez `code` ; le texte humain peut changer :

| Code | Sens |
| --- | --- |
| `invalid_start`, `invalid_ack` | Commande, réglage, type de référence ou compteur invalides. |
| `invalid_control` | Message interactif, génération, révision, occurrence ou limite de connexion invalide. |
| `invalid_seek` | Position au-delà de la fin décodée ou de la limite de 24 heures. |
| `state_failed` | Point de reprise absent, invalide, indisponible, limite de profils atteinte ou échec de sauvegarde/reprise. |
| `state_conflict` | Un autre socket possède ce compte/profil ou l’état sauvegardé a changé en parallèle. |
| `track_not_found`, `catalog_unavailable` | Actualiser catalogue/référence avant de réessayer. |
| `source_changed`, `source_unavailable` | Restaurer ou rescanner la source audio ordinaire. |
| `decode_failed` | Échec du décodage audio, notamment différence de MD5 du contenu FLAC décodé en fin de piste ; conserver le sens d’écoute partielle. |
| `processing_failed`, `stream_failed` | Échec DSP ou travailleur ; conserver le sens d’écoute partielle. |
| `ack_timeout` | Aucun progrès de consommation client à temps. |
| `authentication_expired` | Session expirée/révoquée ou compte ne permettant plus la lecture. |
| `history_failed` | Sauvegarde de l’historique non confirmée. |
| `server_shutdown` | Arrêt du serveur. |

Une panne de transport ou un client fermé ne garantit ni erreur JSON ni confirmation finale. Les restrictions communes des [comptes](accounts.md) et de [HTTPS](remote.md) concernent négociation et flux actif. Les contrôles automatisés PCM/transport ne valident ni gapless physique, qualité du périphérique client ni capacité du NAS cible.
