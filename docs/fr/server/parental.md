# Contrôle parental par compte

La capacité `parental_controls` ajoute des restrictions d’artistes et des budgets d’écoute administrés par compte authentifié. Le catalogue physique reste commun ; les fichiers audio et leurs tags ne sont jamais modifiés. Le mode local anonyme reste inchangé et un administrateur ne peut pas être supervisé. Cette protection concerne Aède et les clients natifs qui respectent son contrat ; ce n’est pas un verrou du système d’exploitation ni une restriction des fichiers ou comptes extérieurs à Aède.

## Administration des règles

Ces routes demandent une session administrateur nominative courante, en HTTPS ou HTTP local. L’activation des règles pose un verrou lié à l’identité stable de cet administrateur. Les autres administrateurs peuvent consulter les règles mais pas les modifier, renommer/promouvoir/désactiver le compte protégé, réinitialiser son mot de passe ou révoquer ses sessions. L’administrateur responsable libère le verrou en désactivant les règles. Les autres administrateurs ne peuvent pas non plus modifier ce responsable ou réinitialiser son mot de passe tant qu’il détient un verrou, afin d’empêcher son usurpation. Il ne peut être désactivé ou rétrogradé tant qu’il détient des verrous actifs. Un jeton local historique ne peut ni prendre ni contourner un verrou nominatif. Cela protège l’API et les comptes, pas les fichiers contre un opérateur local de confiance ou une restauration de sauvegarde par la CLI.

| Méthode et route | Corps | Résultat |
| --- | --- | --- |
| `GET /api/admin/v1/accounts/{username}/parental` | Aucun | `{owner,revision,locked_by,policy}` |
| `PUT /api/admin/v1/accounts/{username}/parental` | `{expected_owner,expected_revision,policy}` | `{owner,revision,locked_by,policy}` enregistré |
| `GET /api/me/v1/parental` | Aucun | État du compte courant |
| `POST /api/me/v1/parental/lease` | Action décrite ci-dessous | Autorisation temporaire ou 204 |

L’identifiant stable `owner` et la révision lus par GET doivent être renvoyés ensemble lors de l’enregistrement. Un compte remplacé ou une modification concurrente produit `409 parental_conflict` ; le client doit relire les règles. Un compte inconnu produit 404. Une cible administrateur, des champs inconnus, des références invalides, des limites excessives ou un fuseau inconnu sont refusés. Le champ `account` de connexion et les métadonnées de session courante incluent toujours `parental_enabled` ; le rôle seul ne suffit pas à déterminer la supervision.

`policy` contient exactement `enabled`, `timezone`, `blocked_artists`, `radio_allowed`, `blocked_podcasts`, `days` et `week_limit_seconds`. `timezone` est un fuseau IANA, par exemple `Europe/Paris`. `blocked_artists` contient au plus 256 références canoniques d’artistes sans préfixe de fédération ; `blocked_podcasts`, au plus 128 adresses RSS HTTP/HTTPS canoniques. `days` contient sept éléments, du lundi au dimanche :

```json
{"windows":[{"start_minute":480,"end_minute":1200}],"limit_seconds":3600}
```

Les fenêtres sont exprimées en minutes locales, début inclus et fin exclue, avec au plus huit fenêtres sans chevauchement par jour. Une fenêtre nocturne doit être répartie sur deux jours. Une liste vide interdit l’écoute ce jour. `null` signifie budget illimité, zéro interdit l’écoute ; les plafonds sont 86400 secondes par jour et 604800 par semaine. Les semaines vont du lundi au lundi dans le fuseau choisi, en tenant compte des changements d’heure. Désactiver les règles conserve leur configuration et leur usage.

L’état personnel est `{owner,revision,policy,day_used_seconds,week_used_seconds,remaining_seconds,allowed,reason,next_change_at}`. Le temps restant vaut null sans budget. `next_change_at` est la prochaine transition du calendrier en secondes Unix. Les motifs sont `outside_schedule`, `daily_limit`, `weekly_limit`, `radio_blocked`, `podcast_blocked`, `artist_blocked` ou `policy_unavailable` ; le motif vaut null si l’accès est autorisé. Cet état musical ne teste pas un artiste précis : la sélection de source le vérifie séparément. La consultation reste possible hors des horaires d’écoute.

## Catalogue et données personnelles

Avant toute projection, une requête supervisée utilise un sous-graphe compact du catalogue sans les artistes bloqués et leurs albums, pistes, enregistrements et crédits concernés. Les compteurs, références directes, images, paroles, formes d’ondes, recherches avancées et sélections personnelles utilisent ainsi le même graphe visible. Connaître une référence stable ne contourne pas le filtre. Deux projections mises en cache bornent la mémoire ; leur identité tient compte de la publication du catalogue, de la révision et des références réellement bloquées. La lecture PCM réutilise ce cache à chaque changement de source, y compris pendant un fondu.

Les données personnelles sont d’abord réconciliées avec le catalogue complet. Tags, notes, étoiles, favoris, collections et historique restent enregistrés ; les éléments masqués réapparaissent si les règles les autorisent de nouveau. Les données d’un autre compte ne sont pas chargées comme celles de l’utilisateur courant. Les identifiants capturés et les règles effectives sont revérifiés avant publication. Une modification des règles invalide une ancienne vue ou lecture en cours.

Les inventaires de dossiers, diagnostics globaux, flux d’événements du catalogue/des tâches, inventaires M3U communs et téléchargements M3U privés originaux sont refusés aux comptes supervisés : ils pourraient révéler des noms/chemins non filtrés ou contourner la lecture contrôlée. Les résumés et sélections de lecture M3U personnels exigent que toutes les pistes référencées restent visibles. La navigation Subsonic est filtrée, mais `stream`, `download` et la récupération des listes y sont indisponibles pour un compte supervisé : le transport original ne contrôle pas la consommation. Un client ne doit pas basculer silencieusement vers ce transport ou une sortie externe/HQPlayer.

## Écoute contrôlée

Le transport PCM réserve une autorisation courte lorsque le format est prêt, avant l’émission du son. Elle dure au plus dix secondes et s’arrête à une limite d’horaire ou de budget. Les identifiants, règles et horaires sont vérifiés pendant l’écoute. Fin, arrêt, déplacement, déconnexion et erreur soldent la réservation. Le PCM supervisé ne peut être émis qu’avec une seconde d’avance au maximum et dans les limites de l’autorisation : des acquittements artificiellement rapides ne permettent pas de télécharger toute la piste. Une lecture initialement non supervisée s’arrête si le compte devient supervisé.

Pour les radios et podcasts lus directement, le client natif doit réserver avant le démarrage, renouveler pendant l’écoute et libérer à la pause, l’arrêt ou la déconnexion :

```json
{"action":"reserve","media":{"kind":"radio"},"seconds":10}
{"action":"reserve","media":{"kind":"podcast","feed":"https://example.invalid/podcast.xml"},"seconds":10}
{"action":"renew","lease_id":"64 caractères hexadécimaux","consumed_ms":1000,"seconds":10}
{"action":"release","lease_id":"64 caractères hexadécimaux","consumed_ms":1000}
```

Réserver/renouveler renvoie `{lease_id,revision,expires_at,granted_seconds,server_now_ms}`. L’expiration est en secondes Unix et l’heure serveur en millisecondes Unix au moment de préparer la réponse. Le client déduit une échéance monotone conservatrice depuis le début de sa requête et le minimum entre la durée accordée et `expires_at * 1000 - server_now_ms` ; il ne suppose pas son horloge murale synchronisée. Chaque renouvellement remet à zéro le compteur cumulatif `consumed_ms` de l’autorisation. Libérer renvoie 204. Les autorisations musicales appartiennent exclusivement au transport PCM. Le serveur génère les identifiants, liés au propriétaire ; un autre compte ne peut ni renouveler ni libérer une autorisation. Le cœur normalise les adresses RSS (schéma, hôte, ports, segments, encodages sûrs et fragments), empêchant le contournement par une autre écriture de la même adresse.

Les budgets comptent l’union des intervalles d’écoute : deux voix de fondu simultanées ne facturent pas deux fois le même temps. Déclarer une consommation nulle ne rembourse pas une autorisation déjà écoulée ; seule sa partie future inutilisée peut être libérée. Une interruption conserve prudemment les réservations en attente. Au plus huit autorisations actives par compte bornent la concurrence. Un court délai réseau/démarrage sous autorisation peut donc compter dans le budget ; il ne s’agit pas d’une mesure physique du son entendu.

Les refus HTTP et PCM utilisent `parental_outside_schedule`, `parental_daily_limit`, `parental_weekly_limit`, `parental_radio_blocked`, `parental_podcast_blocked`, `parental_artist_blocked`, `parental_policy_unavailable` ou `parental_conflict`. Après qu’un processus a observé les règles protégées, leur disparition ou corruption bloque l’accès au lieu de retirer les restrictions ; un administrateur peut toujours se connecter pour la récupération. Une absence initiale signifie qu’aucune règle n’est configurée. Les sauvegardes privées incluent règles et usage ; restaurer une ancienne sauvegarde sans cette partie conserve les restrictions existantes.

`podcasts_allowed` autorise ou interdit tous les podcasts. Quand il est vrai, `blocked_podcasts` bloque uniquement les adresses RSS renseignées. Son absence dans les anciennes règles conserve l’autorisation. Les anciennes règles sans `locked_by` restent sans responsable jusqu’à leur enregistrement par un administrateur nominatif. Désactiver les règles libère le verrou sans supprimer les réglages ni la consommation. Un refus du verrou utilise `parental_locked`.
