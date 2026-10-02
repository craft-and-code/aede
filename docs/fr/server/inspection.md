# Inspection de la bibliothèque et compatibilité

Ces vues sont en lecture seule. Elles ne vérifient pas l’audio, ne réparent pas les tags, ne téléchargent rien et ne font pas de sauvegarde. Les [bases HTTP](http.md) définissent les erreurs et la pagination. Toutes les routes ordinaires ci-dessous acceptent GET et HEAD.

## GET /api/v1/status

Aucun paramètre interprété ni pagination. Vérifie que le processus répond :

```sh
curl http://127.0.0.1:8787/api/v1/status
```

Renvoie `{"status":"ok","api_version":1,"catalog_loaded":true}` avec un catalogue chargé. Cette route reste disponible si le catalogue disparaît ; `catalog_loaded` vaut alors false. `api_version` est la version du contrat HTTP, indépendante des versions du programme et des fichiers de données. Une connexion refusée indique généralement un serveur arrêté ou un mauvais port ; ce n’est pas une erreur JSON de l’API.

### HEAD /api/v1/status

`curl -I http://127.0.0.1:8787/api/v1/status` vérifie la disponibilité HTTP sans corps JSON.

## GET /api/v1/library

Aucun paramètre interprété ni pagination.

```sh
curl http://127.0.0.1:8787/api/v1/library
```

Renvoie `scanned_at,files,artists,releases,recordings,tracks`. Ce sont des nombres d’entités/fichiers : pistes et enregistrements peuvent différer, car une interprétation peut avoir plusieurs positions locales. La date est en secondes Unix. Catalogue indisponible : `503 catalog_unavailable`.

### HEAD /api/v1/library

Même vérification du catalogue chargé, statut/en-têtes seulement.

## GET /api/v1/doctor

Paramètres : `severity=error|warning|info` facultatif, `offset`, `limit`.

```sh
curl 'http://127.0.0.1:8787/api/v1/doctor?severity=warning&limit=20'
```

Renvoie une page complétée par `summary`, `unverified_files`, `pending_analyses`. `summary` compte tous les problèmes correspondant au filtre avant pagination, par gravité. Chaque problème contient `type,label,severity,detail,files,file_count,files_truncated`. `type` est un nom exploitable par un logiciel ; `label` et `detail` l’expliquent. `files` montre au plus 20 chemins ; `file_count` donne le nombre complet et `files_truncated` signale une liste tronquée.

Doctor lit catalogue, conclusions et sources actuels sous verrou, y compris les vérifications d’intégrité enregistrées depuis le dernier scan. Il constate, sans réparer. `unverified_files` signifie absence de verdict d’intégrité enregistré, pas automatiquement fichier endommagé. `pending_analyses` concerne les analyses en attente. `409 store_busy` indique une écriture en cours : réessayez après sa fin. Une gravité invalide donne 400 ; catalogue/conclusions/sources illisibles donnent 500, jamais un bilan propre trompeur.

### HEAD /api/v1/doctor

Même validation/inspection sans transmettre les problèmes. HEAD ne lance pas non plus de vérification audio.

## GET /api/v1/stats

Seulement `offset`, `limit`. Aucun tri ou filtre textuel.

```sh
curl 'http://127.0.0.1:8787/api/v1/stats?limit=10'
```

Les champs principaux sont `scanned_at,files,tracks,albums,compilations,artists,album_artists,labels,genres,duration_ms,bytes,tracks_without_album,completeness`. La complétude donne `covers,years,genres,mbid` sous forme de proportions, pas de pourcentages. `by_codec,by_quality,by_sample_rate,by_decade,by_country,roles` et `top.artists,top.writers` sont des tableaux paginés séparément : les mêmes offset/limit s’appliquent à **chacun**, pas aux totaux principaux.

Les lignes ordinaires contiennent `label,count,bytes` ; les groupes pays comptent les artistes, et `bytes:0` signifie non mesuré, pas absence de fichiers. Les rôles contiennent `role,artists,credits` ; les artistes/auteurs les plus représentés contiennent `reference,name,tracks`. Durées/octets proviennent du catalogue local. Sources illisibles : `sources_unavailable` ; travailleurs occupés : `429 inspection_busy`.

### HEAD /api/v1/stats

Valide et calcule la même vue sans tableaux/corps de réponse.

## GET /api/v1/roots

Seuls `offset`, `limit` sont acceptés.

```sh
curl http://127.0.0.1:8787/api/v1/roots
```

Renvoie une page et les `totals:{tracks,duration_ms,bytes}` de toute la bibliothèque. Les lignes contiennent `status,path,tracks,duration_ms,bytes`. `watched` est une racine suivie, `excluded` une racine exclue enregistrée ; la ligne synthétique `unwatched` rassemble les pistes hors racines suivies, avec `path:null`. Des racines imbriquées peuvent compter une piste dans plusieurs lignes ; les totaux globaux la comptent une fois. Cette route ne change pas les racines et ne rescane rien. Paramètres non pris en charge : 400.

### HEAD /api/v1/roots

Même inspection sans liste de racines dans la réponse.

## GET /api/v1/roles

Seuls `offset`, `limit` sont acceptés.

```sh
curl http://127.0.0.1:8787/api/v1/roles
```

Chaque élément est `{role,artists,credits}` pour un rôle réellement présent. `artists` compte les artistes concernés ; `credits` compte les crédits de ce rôle. Cette route ne liste pas tous les rôles possibles et ne filtre pas les artistes par rôle. Filtre non pris en charge : 400.

### HEAD /api/v1/roles

Même validation/vue, sans corps.

## GET /api/v1/countries

Paramètres : `q` facultatif (nom de pays/zone, code ISO ou initiales dérivées par Aède), `offset`, `limit`. Ordre fixe par nombre d’artistes puis nom ; aucun autre tri.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/countries' --data-urlencode 'q=France'
```

La page ajoute `source,matched_by,coverage`. Les lignes contiennent `name,iso_code,derived_initials,artists,tracks,duration_ms,bytes`. `iso_code` est un code officiel lorsqu’il existe ; `derived_initials` aide la navigation sans être un code officiel. Une zone peut être une région. Les faits viennent de MusicBrainz déjà enregistré, pas du pays de pressage ni d’un nouveau téléchargement. `matched_by` vaut `exact`, `partial` ou null sans filtre.

La couverture contient `artists_total,artists_asked,artists_with_area,artists_without_area,places_without_iso_code` et garde visibles les origines inconnues. Un `q` sans zone connue correspondante produit `400 invalid_query`, contrairement aux listes ordinaires vides. Une source illisible donne 500, pas une prétendue absence d’origine pour tous.

### HEAD /api/v1/countries

Même recherche, calcul de couverture et erreurs, sans JSON.

## GET /api/v1/years

Seulement `offset`, `limit` ; ordre chronologique sans autre tri.

```sh
curl 'http://127.0.0.1:8787/api/v1/years?limit=20'
```

La page ajoute `albums_total,albums_without_year,tracks_without_dated_album`. Les lignes contiennent `year,albums,tracks,duration_ms,bytes`. L’année est celle enregistrée pour l’album. Les années inconnues sont comptées séparément, sans année zéro inventée. La couverture des pistes inclut aussi celles sans album daté. Tri/filtre non pris en charge : 400.

### HEAD /api/v1/years

Même vue chronologique/statut sans corps.

## GET /api/v1/releases

Liste d’origine conservée pour les clients version 1. Préférez `/albums` pour les filtres d’artistes/genres/labels par nom. Paramètres : `q` (titre), `artist` (**référence d’artiste seulement**), `year`, `sort=catalog|title|year`, `order`, `offset`, `limit`.

```sh
curl 'http://127.0.0.1:8787/api/v1/releases?year=1980&sort=title'
```

Les lignes contiennent `reference,title,year,album_artist,track_count,cover_path`, comme dans [Albums](catalog.md). Le filtre artiste porte seulement sur l’artiste d’album. `artist=AC/DC` n’est pas valide ici : obtenez d’abord une référence d’artiste. Référence mal formée/du mauvais type : 400 ; artiste valide absent : 404.

### HEAD /api/v1/releases

Même filtres/statut de compatibilité, sans JSON.

## GET /api/v1/entities

Un seul paramètre `ref` obligatoire ; aucun sélecteur par nom ni pagination.

```sh
# Remplacez cette valeur illustrative par une référence d’une réponse précédente.
curl --get 'http://127.0.0.1:8787/api/v1/entities' --data-urlencode 'ref=artist:REFERENCE_FROM_RESPONSE'
```

Renvoie un détail générique avec `kind,reference` et les champs de son type. Artiste : `name,sort_name,mbid,aliases,releases`. Album : `title,year,album_artist,tracks,release_group,labels,cover_path`. Piste : `title,release,recording,duration_ms,path,size,analyses`. Enregistrement : `title,isrc,mbid,tracks,works`. Œuvre : `title,mbid,recordings`. Groupe de sorties : `title,mbid,releases`. Label : `name,mbid,releases`. Genre : `name,releases,tracks`.

Contrairement à `/artist`, le détail artiste générique n’ajoute pas `origin`. Les liens sont des références, leurs listes sont complètes. Référence absente/mal formée : `400 invalid_reference` ; entité valide absente : 404. La route inspecte une entité du graphe, pas un chemin arbitraire du disque.

### HEAD /api/v1/entities

Valide/résout la même référence sans son détail.

Le [contrat d’API](../../api.md) précise les schémas techniques. Suite : [Recherche et query](search.md).
