# Annotations, historique et collections personnels

Ces routes administratives exigent [l’authentification d’administration](administration.md) et visent `local`. Seuls le jeton de confiance de l’installation ou une session administrateur propriétaire de `local` peuvent les employer ; les autres sessions administrateur reçoivent `403 forbidden`. Les [sessions de comptes](accounts.md) emploient les mêmes opérations sous `/api/me/v1`, liées à leur propriétaire authentifié. Aucune requête ne choisit un autre propriétaire. Une session `auditor` peut employer seulement les formes `GET` et `HEAD` ; toute opération personnelle `PUT`, `POST` ou `DELETE` renvoie `403 forbidden`. Catalogue sur disque et `user.json` sont lus sous verrou pour valider et publier dans une même version complète.

## GET /api/admin/v1/annotation

`ref` obligatoire, désignant une entité actuelle ; sans corps ni pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/admin/v1/annotation' \
  --data-urlencode 'ref=artist:REFERENCE_FROM_RESPONSE' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Remplacez le jeton illustratif par une vraie référence. Renvoie HTTP 200 `{reference,annotation}`. Annotation vaut null sans valeur personnelle ; sinon ses champs sont `reference,loved,rating,note,tags,created_at,updated_at`. Les dates sont en secondes Unix ; rating/note absents sont null et tags est un tableau. Les lectures publiques `/artist` ou `/track` n’exposent jamais ces valeurs privées. Référence invalide : 400 ; entité valide absente : 404.

### HEAD /api/admin/v1/annotation

Même validation de secret/référence/verrou sans corps d’annotation.

## PUT /api/admin/v1/annotation

Sélecteur `ref` obligatoire dans l’URL ; patch JSON avec au moins l’un de ces champs :

| Champ | Valeur acceptée | Sens |
| --- | --- | --- |
| `loved` | booléen true/false | Ajoute/retire le favori ; null invalide. |
| `rating` | entier 1–5 ou null | Étoiles ; null retire la note. |
| `note` | texte non réduit à des espaces, ou null | Note Markdown personnelle ; null la retire. C’est du texte stocké, pas du HTML exécutable. |
| `tags` | tableau de textes ou null | Remplace **tous** les tags ; null/tableau vide efface. Au plus 100 tags uniques non vides de 256 octets chacun, sans NUL ; doublons refusés. |

Un champ absent reste inchangé. `{}` est refusé. Les tags de réponse sont triés. Sans favori/étoiles/note/tag restants, l’annotation est supprimée plutôt que conservée vide. Aucun tag/fichier audio ne change.

Utilisez un client séparant paramètres d’URL et corps JSON. Avec curl, placez la référence déjà encodée pour URL dans `AEDE_REF_URL`, puis :

```sh
curl -X PUT "http://127.0.0.1:8787/api/admin/v1/annotation?ref=$AEDE_REF_URL" \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' -d '{"loved":true,"rating":5}'
```

Copiez la référence d’une réponse précédente et encodez-la avant de définir cette variable. Ne combinez pas `--get` avec le corps JSON d’une mise à jour : curl pourrait déplacer le corps dans l’URL. Renvoie HTTP 200 avec le `{reference,annotation}` résultant. Types incorrects/champs inconnus : `invalid_body` ; plages invalides, loved null, note vide ou tags invalides : `invalid_parameters`. Les conflits/erreurs de stockage ci-dessous interviennent avant sauvegarde.

## GET /api/admin/v1/history

Paramètres : `offset`, `limit`, sans corps. Liste les événements récents du plus récent au plus ancien.

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/history?limit=20' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Renvoie HTTP 200 et une page de `track,at,ms_played,completed,play_count,last_played`. Dates at/last_played en secondes Unix ; durée en millisecondes. Les compteurs sont les totaux historiques du propriétaire par piste, pas ceux de cette page. Chaque propriétaire conserve ses 500 événements natifs les plus récents ; les écoutes d’un autre propriétaire ne les suppriment pas. Tri par date ; dates égales distinctes, avec dernier événement reçu en premier. Les compteurs survivent à la sortie d’anciens événements du journal de ce propriétaire.

### HEAD /api/admin/v1/history

Même validation authentifiée de page/verrou sans historique JSON.

## POST /api/admin/v1/history

Aucun paramètre. JSON : `track` obligatoire (référence actuelle de piste), `ms_played` obligatoire (entier non signé, maximum 24 heures = 86400000), `completed` obligatoire (booléen), `at` facultatif (secondes Unix, heure serveur par défaut, au plus un jour dans le futur).

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/history' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"track":"track:REFERENCE_FROM_RESPONSE","ms_played":180000,"completed":true}'
```

Remplacez la référence illustrative. Renvoie HTTP 201 avec l’événement et son compte total actualisé. La route **enregistre ce que le client déclare** ; elle ne joue pas la musique et ne vérifie pas l’écoute. Chaque POST accepté incrémente le compteur, même pour une date trop ancienne pour le journal récent. Il n’est pas idempotent : répéter une soumission perdue peut compter deux fois. Mauvais type/référence : 400 ; piste absente : 404 ; durée/date hors limites : 400. Aucune diffusion audio ne se cache derrière cette route.

## GET /api/admin/v1/collection

`name` non vide obligatoire ; sans corps ni pagination. Comparaison sans distinction de casse/accents.

```sh
curl --get 'http://127.0.0.1:8787/api/admin/v1/collection' \
  --data-urlencode 'name=Favoris' -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Renvoie HTTP 200 `{name,expression,created_at,updated_at}` ; dates en secondes Unix. C’est l’**expression** enregistrée, pas sa liste actuelle de pistes. Nom inconnu : `404 collection_not_found`.

### HEAD /api/admin/v1/collection

Même validation de nom/jeton/statut, sans corps.

## PUT /api/admin/v1/collection

`name` obligatoire (1–256 octets) ; JSON `{expression:"…"}` avec expression non vide de 1–2048 octets. Sa grammaire est validée avant sauvegarde. Un nom normalisé existant est remplacé, un nouveau créé. La collection intelligente évolue avec bibliothèque/valeurs personnelles, contrairement à une playlist statique.

```sh
curl -X PUT 'http://127.0.0.1:8787/api/admin/v1/collection?name=Favoris' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' -d '{"expression":"loved"}'
```

Renvoie HTTP 200 `{name,expression,created_at,updated_at}`. Expression/taille invalides : `400 invalid_parameters` ; nom invalide : `invalid_query` ; champ/type JSON : `invalid_body`. Les clauses personnelles peuvent figurer dans l’expression de ce propriétaire ; `/query` public les refuse toujours. La sauvegarde n’exporte pas de M3U et ne lance pas la lecture.

## DELETE /api/admin/v1/collection

`name` obligatoire, sans corps. Supprime explicitement la collection, en conservant musique et annotations.

```sh
curl -X DELETE 'http://127.0.0.1:8787/api/admin/v1/collection?name=Favoris' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Renvoie HTTP 204 sans corps. Collection absente : `404 collection_not_found`. C’est une suppression effective, pas une prévisualisation.

## GET /api/admin/v1/collections

Seulement `offset`, `limit`, sans corps.

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/collections?limit=20' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Renvoie HTTP 200 et une page de `{name,expression,created_at,updated_at}`. Liste vide : aucune collection enregistrée. Filtre inconnu : 400. Lister n’exécute aucune expression.

### HEAD /api/admin/v1/collections

Même validation authentifiée de pagination/stockage, sans JSON.

## Erreurs communes et récupération

Toutes ces lectures/écritures prennent le verrou du dossier. Un autre rédacteur produit `409 store_busy` : réessayez après sa fin. Catalogue absent : `503 catalog_unavailable` ; catalogue illisible : `500 store_error` ; données personnelles corrompues/illisibles : `500 user_unavailable` ; échec de travail inattendu : `500 personal_failed`. Ces échecs de chargement/validation ne sauvegardent aucune donnée personnelle. Préservez fichiers abîmés et sauvegardes avant réparation plutôt que réinitialiser. Le [guide de sauvegarde](../../operating.md#backups-and-recovery) décrit la récupération.

Le journal natif `Play` et le journal séparé `Scrobble` de Subsonic conservent chacun au maximum 500 événements par propriétaire. Leur capacité globale indépendante est de 128 000 événements par journal (500 × la capacité de 256 comptes). L’entrée de `user.json` et sa représentation sérialisée indentée sont limitées à 64 Mio, échappements et espaces compris. Chargement/import et sauvegarde refusent une borne dépassée ; ils ne réduisent pas les données d’un autre propriétaire et ne remplacent pas le fichier existant. Les échecs HTTP de chargement utilisent `user_unavailable`, ceux de publication `store_error`. Ces budgets concernent le stockage, pas la mémoire totale du processus : le document parsé, les valeurs du domaine et la représentation JSON occupent aussi de la mémoire. Aucun changement de format ni migration automatique ; les compteurs sont conservés et les événements déjà supprimés ne peuvent être reconstitués.

Les playlists statiques n’ont pas encore de route native `/api/me/v1`, mais l’[adaptateur Subsonic/OpenSubsonic](subsonic.md) lit et écrit des playlists privées ordonnées dans le même fichier personnel. Il sert également les pochettes JPEG/PNG cataloguées à côté des fichiers et les fichiers audio originaux, et enregistre les scrobbles déclarés séparément des événements `Play` : aucune durée écoutée ni fin de lecture n’est inventée. Les deux types alimentent les compteurs historiques par piste de leur propriétaire ; la page native d’historique liste uniquement les événements `Play`. Une déclaration temporaire de lecture en cours, privée à son propriétaire, n’ajoute ni compteur ni historique.

Annotations de relations, review des sources, check/analyze/fingerprint, copy, backup/restore, reset, merge, inspection arbitraire de fichiers et commandes shell n’ont pas de routes HTTP actuelles. Paroles/textes restent hors des contrats HTTP. Leur existence en CLI ne les rend pas disponibles par API.

## Enregistrements M3U répétés

La liste M3U regroupe les anciens enregistrements uniquement si le compte, le nom exact et les références de pistes dans leur ordre sont identiques. Elle fournit l’identifiant du plus récent et le nombre `duplicates`. Les autres noms, ordres et comptes restent distincts. Les fichiers et les enregistrements existants sont conservés. Un nouvel enregistrement identique réutilise déjà la sélection et recrée un export absent. La capacité `grouped_m3u_exports` annonce ce comportement.

`GET /api/me/v1/m3u` accepte `offset`, `limit` et le paramètre facultatif `q`. La recherche utilise la normalisation existante, sans distinction de casse ni d’accents, sur les noms de tous les exports regroupés du compte authentifié, avant pagination. `q` doit contenir du texte recherchable et respecter 2048 octets UTF-8. La capacité `library_m3u_files` annonce ce nouveau paramètre ; les clients qui parlent à un ancien serveur doivent l’omettre. Les identifiants privés, la publication et les téléchargements gardent leur contrat existant.

## Fichiers M3U présents dans les dossiers surveillés

La capacité `library_m3u_files` annonce deux nouvelles routes authentifiées en lecture seule. Elles exposent des fichiers partagés de la bibliothèque, séparément des exports privés enregistrés ; aucun fichier n’est importé dans `user.json` ni réécrit. Toute session de compte, y compris un auditeur, peut les lire. Un jeton administratif seul n’autorise pas `/api/me/v1`.

Le sous-arbre de publication privé `data_dir/playlists` est toujours exclu implicitement de la liste et du détail des fichiers, indépendamment des exclusions configurées, même si un dossier surveillé englobe le dossier de données. Un identifiant de fichier calculé à partir du chemin ne contourne pas cette limite. Les résumés et téléchargements existants `/api/me/v1/m3u` continuent d’exposer uniquement les exports privés du compte authentifié.

| Route | Paramètres | Réponse |
| --- | --- | --- |
| `GET /api/me/v1/m3u-files` | `offset`, `limit`, `q` facultatif ; aucun corps | Page habituelle `{items,total,offset,limit,scanned_at}`, plus `unavailable_roots` et `unreadable_directories`. Éléments `{id,name,filename,path,track_count:null}`. |
| `GET /api/me/v1/m3u-files/{id}` | Aucun paramètre ni corps | `{id,name,filename,path,track_count,content}`. |

La pagination utilise 50 éléments par défaut et accepte 1 à 200. La recherche applique la même validation que les exports privés, correspond au chemin canonique complet, nom de fichier compris, et précède la pagination. Le tri suit les chemins canoniques, indépendamment de l’ordre d’énumération du système de fichiers. La découverte inclut chaque fichier régulier `.m3u` ou `.m3u8`, sans distinction de casse, même dans les dossiers sans audio catalogué. Les alias canoniques et les dossiers surveillés imbriqués sont dédupliqués. Les sous-arbres exclus et les symlinks enfants, fichiers ou dossiers, sont omis ; une racine surveillée peut elle-même être un alias canonique de dossier. Un chemin de playlist non UTF-8 ou dépassant 4096 octets UTF-8 produit une erreur explicite plutôt qu’une liste tronquée ou une conversion avec perte.

La liste n’ouvre pas le contenu des playlists. Les fichiers invalides ou trop volumineux restent donc visibles et peuvent produire une erreur à la sélection. Le détail conserve exactement le texte UTF-8, BOM, CRLF, espaces et extension d’origine compris. Les deux rendus CLI partagés, M3U étendu et `playlist --simple`, sont acceptés. `track_count` compte les lignes d’entrée non vides et non commentées, répétitions et URL comprises ; il n’affirme pas qu’elles sont lisibles. Les chemins, directives et URL contenus dans la playlist ne déclenchent aucune résolution de fichiers, lecture audio ni requête réseau. Les contenus binaires ou caractères de contrôle autres que tabulation/CR/LF sont refusés. Les fichiers anciens dans un autre encodage sont listés mais leur lecture nécessite une conversion explicite par l’utilisateur, hors de cette API.

Le parcours est limité à 10 000 playlists, 250 000 entrées du système de fichiers, une profondeur de 64 et une échéance coopérative de trois secondes. Une limite refuse la réponse plutôt que présenter un total partiel comme complet. Les racines absentes et les opérations/entrées de dossiers inaccessibles alimentent les deux compteurs de diagnostic ; une page réussie peut donc décrire explicitement le sous-ensemble accessible. Le détail est limité à 8 Mio d’entrée et 4096 lignes d’entrée ; le JSON sérialisé, échappements compris, à 16 Mio. Les inspections utilisent les emplacements de workers bornés existants et conservent leur emplacement après déconnexion du client. L’échéance ne peut pas interrompre un appel système déjà bloqué, notamment sur un système de fichiers réseau.

L’identité `m3u-file-` suivie de 64 caractères hexadécimaux minuscules est un sélecteur de chemin déterministe, pas un jeton d’autorisation. Le détail redécouvre son identité sous les racines et exclusions actuelles et refuse les identités ambiguës. Un déplacement ou renommage peut changer l’identité. La révocation de session et les racines/exclusions/version du scan sont revérifiées avant publication ; les alias de racines et les métadonnées du chemin/fichier sélectionné sont également contrôlés autour des lectures. Ces contrôles détectent les remplacements ordinaires et changements de périmètre sans constituer un confinement du système de fichiers par descripteurs : un échange hostile et concurrent de chemins/symlinks conserve des limites TOCTOU. Les vérifications Unix incluent le périphérique et l’inode ; les contrôles de taille/type/date offrent une détection plus faible sur les autres plateformes. Aucune garantie supérieure de course sur Windows n’est annoncée.

Une identité malformée ou un paramètre non pris en charge donne `400 invalid_query` ; un fichier absent des racines autorisées, `404 m3u_file_not_found` ; un remplacement/changement de périmètre détecté, `409 m3u_files_changed` ; une limite de parcours/contenu/entrées/réponse, `413 m3u_files_too_large` ; un chemin ou contenu non pris en charge, `422 m3u_path_invalid` / `m3u_text_invalid` ; une lecture indisponible ou une échéance atteinte, `503 m3u_file_unavailable` / `m3u_files_timeout`. Les erreurs existantes de saturation des workers personnels et de session s’appliquent également. Actualiser ou réessayer reste explicite ; aucune erreur ne réécrit les fichiers sources ou le stockage privé.

## Préférences de présentation et ambiances du compte

`GET /api/me/v1/preferences` renvoie `{values:{clé:valeur-JSON}}` pour le compte authentifié. `PATCH` reçoit `{values:{...}}` et fusionne uniquement les clés fournies. Les clés commencent par `phemios.`, sont limitées à 128 caractères ASCII alphanumériques ou `._:-` ; chaque valeur JSON est limitée à 8192 octets. Un compte conserve au maximum 64 clés et une modification contient 1 à 32 clés. Les auditeurs ne peuvent que lire. Ces opérations ne nécessitent aucun catalogue scanné, revérifient le compte sous verrou et préservent les autres propriétaires et annotations. Un stockage corrompu est refusé. Le champ optionnel `client_preferences` de `user.json` reste compatible avec les anciens fichiers. La capacité `account_preferences` annonce le contrat ; aucun secret ni dossier d'installation n'y est enregistré.

`GET /api/me/v1/moods` renvoie `{moods:[{rule,count}],items,total,offset,scanned_at}`. `mood` sélectionne une règle ; la pagination personnelle est limitée à 200 éléments. `PUT` reçoit `{id,name,tags,genres}` pour créer ou modifier une règle privée. Au maximum 32 règles personnalisées, chacune avec 32 tags et 32 genres, sont conservées dans 8192 octets de JSON. Une règle inconnue en lecture, des critères vides ou invalides sont refusés.

Le crate autonome `aede-moods`, sans dépendance externe, sélectionne par tags et genres normalisés exacts. Une piste hérite de ses propres annotations, de celles de son album et de l'artiste principal de l'album ; les invités et les autres comptes restent exclus. Les liens de genres utilisent les mêmes cibles. Un tag personnel reconnu sur la piste prime sur ceux de l'album, puis de l'artiste principal. Le vocabulaire comprend les règles personnalisées et les suggestions modifiées par ce compte. Les tags sans rapport avec une ambiance ne bloquent pas les critères hérités. Lorsqu'un tag personnel reconnu existe, les genres ne sélectionnent pas une ambiance contradictoire ; sinon les genres exacts restent le recours. Chaque piste expose `{reason:{tag,genre,scope}}` et les champs de sélection de lecture habituels. `scope` vaut `track`, `album`, `artist` ou `genre`. Les résultats sont classés par précision de cette source avant pagination ; à source égale, l'ordre chronologique des albums et leurs positions de pistes est conservé. Cette priorité ne représente pas un score de confiance ou une émotion mesurée. Les règles sont conservées dans la préférence privée `phemios.moods`. Les suggestions initiales ne sont pas des émotions mesurées ; aucune lecture ne contacte un fournisseur ni ne lance une analyse. La capacité `mood_rules` annonce ces routes.

## Recherche avancée et listes ordonnées

`GET /api/me/v1/search` reçoit `expression` (8192 octets maximum), `offset` et `limit` (1 à 200). La recherche utilise la grammaire des collections avec les annotations du seul compte authentifié, sous le verrou du catalogue et du stockage personnel. Le résultat est une page de pistes légère. Un auditeur peut la consulter ; aucun paramètre ne choisit un autre compte. La capacité `advanced_search` annonce ce contrat. `bitdepth`, `channels` et `container` interrogent les propriétés déclarées des fichiers ; une valeur absente reste inconnue. `ref:=track:…` conserve l’identité d’une piste dans une collection.

`POST /api/me/v1/m3u` accepte aussi `{name,tracks:["track:…",…]}` au lieu de `release`. Un seul sélecteur est permis. La liste doit être non vide, contient au maximum 4096 occurrences et respecte la limite existante de 16 Kio du corps. L’ordre et les pistes répétées sont conservés. Une piste absente ou des sélecteurs mélangés sont refusés. La publication dans les dossiers musicaux exige toujours un album et un administrateur. La capacité `queue_m3u` annonce cette extension. Une collection conserve ses critères ou les identités choisies ; le M3U conserve également l’ordre et les répétitions.

## Trophées masqués

`GET /api/me/v1/achievements` présente la progression du compte courant. `POST` avec `{}` conserve les nouveaux déblocages ; `{ "advanced_search": true }` déclare une recherche avancée réussie côté client. Les auditeurs ne peuvent que lire. La capacité `private_achievements` annonce ce contrat. Les entrées verrouillées exposent uniquement un emplacement anonyme et une date nulle ; leurs définitions restent dans le moteur. Les dates de déblocage sont conservées, à partir des données privées disponibles, dont les écoutes terminées et confirmées. Les données internes réservées sont exclues de `/api/me/v1/preferences`. Un stockage illisible est conservé ; un échec de cette projection facultative ne peut pas refuser une annotation ou une écriture d’historique valide. Les comptes locaux et distants ont leur propre progression.


Les pages de collections comprennent aussi `duration_ms` lorsque toutes les pistes correspondant à la règle ont une durée connue. Le calcul utilise le propriétaire authentifié et le même catalogue verrouillé, uniquement pour la page demandée. Une durée inconnue, une ancienne règle invalide ou un dépassement arithmétique omet cette valeur ; une sélection vide connue vaut zéro. Aucun fichier source ni aucune annotation ne change.
