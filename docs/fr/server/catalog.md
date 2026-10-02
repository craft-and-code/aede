# Albums, artistes, pistes et enregistrements

Ces routes consultent le catalogue local actuel. Elles ne téléchargent aucune information, ne changent pas les tags et ne lancent pas la lecture. Voir les [bases HTTP](http.md) pour pagination, encodage, ambiguïtés et erreurs. Chaque GET ci-dessous renvoie HTTP 200 en cas de réussite ; son HEAD renvoie les mêmes statut/en-têtes sans corps.

## GET /api/v1/albums

Liste les albums, avec une ligne par sortie/édition locale. Paramètres facultatifs : `q` **ou** `name`, `artist`, `year`, `genre`, `label`, `mbid`, `sort`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/albums' \
  --data-urlencode 'artist=AC/DC' --data-urlencode 'year=1980' \
  --data-urlencode 'sort=title' --data-urlencode 'limit=10'
```

`q`/`name` recherche un fragment normalisé du titre. Artiste, genre et label acceptent un nom partiel ou une référence du bon type. Artiste signifie **artiste d’album**, pas tout musicien crédité. Le genre examine les liens d’album et de pistes. Les filtres se combinent avec ET ; un artiste/genre/label inconnu produit une erreur. `year` est un entier exact non signé sur 32 bits ; `mbid` est un identifiant exact sensible à la casse. Les tris par titre acceptent `sort=name|title` ; `year` et `catalog` sont aussi disponibles, avec `order=asc|desc`.

Les `items` contiennent `reference,title,year,album_artist,track_count,cover_path`. Suivez `album_artist` avec la route artiste. `cover_path` est un chemin local, pas une adresse de téléchargement d’image. `null` indique une valeur inconnue. Deux éditions peuvent partager un titre : utilisez leurs références pour le détail. Les paramètres invalides/répétés renvoient `400 invalid_query` ou une erreur de pagination ; une recherche sans album renvoie normalement une page vide.

### HEAD /api/v1/albums

Même validation des filtres. `curl -I 'http://127.0.0.1:8787/api/v1/albums?limit=10'` vérifie la réponse sans télécharger le JSON.

## GET /api/v1/album

Choisit un album avec exactement `name` ou `ref`, sans pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/album' --data-urlencode 'name=Back in Black'
```

L’objet contient `kind:"release",reference,title,year,album_artist,tracks,release_group,labels,cover_path`. `tracks` est la liste complète de références de pistes, pas des fichiers audio ni des objets imbriqués. Consultez-les avec `/track`. `release_group` relie les éditions du même album ; `labels` relie ses labels. Les valeurs inconnues sont null et les liens absents des tableaux vides. Un titre multiple produit `409 ambiguous_entity` : choisissez une référence proposée. Un album absent produit `404 entity_not_found`.

### HEAD /api/v1/album

Accepte le même sélecteur unique et renvoie statut/en-têtes sans détail. Les albums absents ou ambigus restent en 404/409.

## GET /api/v1/artists

Parcourt les artistes avec `q`, `mbid` exact, `sort=catalog|name`, `order`, `offset`, `limit`. La recherche examine noms et alias enregistrés ; le tri `name` utilise `sort_name`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/artists' \
  --data-urlencode 'q=Miles' --data-urlencode 'sort=name'
```

Chaque élément est `{reference,name,sort_name,mbid,aliases}`. Les alias forment un tableau, éventuellement vide. `mbid:null` indique l’absence d’identifiant local. Sans résultat, la page est vide ; les paramètres non pris en charge sont refusés. Les options CLI `--role` et `--country` ne sont pas des paramètres de cette route : utilisez `/roles` ou `/countries` pour examiner ces dimensions.

### HEAD /api/v1/artists

Valide la même liste sans renvoyer sa page, par exemple `curl -I 'http://127.0.0.1:8787/api/v1/artists?limit=1'`.

## GET /api/v1/artist

Choisit un artiste avec exactement `ref` ou `name` ; ses alias sont acceptés. Aucune pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/artist' --data-urlencode 'name=Miles Davis'
```

Le détail contient `kind:"artist",reference,name,sort_name,mbid,aliases,releases,origin`. `releases` est la liste complète des références d’albums. `origin` est un fait enregistré et attribué, pas une déduction du pays de pressage d’un album. Ses champs sont décrits sous `/from`. Ce détail ne reproduit pas chaque panneau CLI, biographie ou vue de relations. Un nom ambigu produit 409 avec candidats ; un artiste absent produit 404.

### HEAD /api/v1/artist

Vérifie le même choix avec statut/en-têtes seulement ; aucun JSON de détail n’est envoyé.

## GET /api/v1/from

Consulte uniquement l’origine d’un artiste choisi par exactement `ref` ou `name`, sans pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/from' --data-urlencode 'name=Miles Davis'
```

La réponse est `{reference,name,origin}`. Une origine inconnue est explicitement représentée :

```json
{"status":"unknown","country_code":null,"area":null,"message":"No MusicBrainz information has been fetched for this artist; GET never fetches it automatically.","attribution":null}
```

Cet exemple représente l’objet `origin`, pas toute la réponse. `status:"known"` signifie qu’un code pays ou une zone est disponible ; l’autre champ peut rester null. Une zone peut être une région. L’attribution contient `source,source_id,fetched_at,confidence,match_score,trusted` ; la confiance est `identified` ou `matched`, avec un score null pour une identification exacte. Seules les identités MusicBrainz enregistrées, fiables et non contradictoires établissent l’origine. GET ne recherche jamais un fait manquant. Le texte humain `message` n’est pas un code stable. Les erreurs de choix sont celles de `/artist` ; une source illisible produit `500 sources_unavailable`, pas une fausse origine inconnue.

### HEAD /api/v1/from

Effectue les mêmes validations de choix et de sources sans corps d’origine.

## GET /api/v1/tracks

Liste les positions de pistes avec `q` (fragment de titre), `release` (une **référence** d’album), `sort=catalog|title`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/tracks' \
  --data-urlencode 'q=Hells Bells' --data-urlencode 'limit=10'
```

Les éléments contiennent `reference,title,release,recording,duration_ms`. La durée est en millisecondes ; null signifie inconnue. Un titre d’album ne convient pas pour `release` : obtenez d’abord sa référence avec `/album`. Une référence mal formée/du mauvais type produit 400 ; une référence valide mais absente produit 404. Aucun titre correspondant donne une page vide. Une piste peut ne pas être liée à un album ou enregistrement.

### HEAD /api/v1/tracks

Même filtres/statut sans JSON. `curl -I 'http://127.0.0.1:8787/api/v1/tracks?limit=1'` vérifie cette route.

## GET /api/v1/track

Choisit une piste avec exactement `ref` ou `name`, sans pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/track' --data-urlencode 'name=Hells Bells'
```

Le détail contient `kind:"track",reference,title,release,recording,duration_ms,path,size,analyses`. `path` est le chemin du fichier musical local ; `size` est en octets. `analyses` contient les résultats acoustiques attribués : `source,source_version,imported_at,stale`, les champs de mesure et `source_data`. Les données complètes de l’analyse d’origine sont conservées dans `source_data` si disponibles ; d’anciens imports peuvent y avoir null. `stale` repère un résultat qui ne correspond plus aux faits du fichier actuel. Ces analyses restent séparées des tags locaux. La route n’analyse pas le fichier et ne diffuse pas son audio. Les titres identiques existent souvent sur plusieurs éditions : après 409, sélectionnez une référence. Une piste absente produit 404.

### HEAD /api/v1/track

Valide le même choix sans détail de fichier/analyse. HEAD ne lit ni ne transmet l’audio.

## GET /api/v1/recordings

Liste les interprétations enregistrées avec `q` (titre), `work` (référence d’œuvre), `sort=catalog|title`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/recordings' --data-urlencode 'q=Hells Bells'
```

Les éléments contiennent `reference,title,mbid,track_count,work_count`. Une même interprétation peut apparaître sur plusieurs éditions locales, donc sur plusieurs pistes. `work_count` compte les compositions liées, pas les fichiers. Le filtre d’œuvre suit des liens explicites du catalogue ; référence mal formée/du mauvais type : 400, référence valide absente : 404. Sans résultat, la page est vide.

### HEAD /api/v1/recordings

Même validation de liste, statut/en-têtes seulement : `curl -I 'http://127.0.0.1:8787/api/v1/recordings?limit=1'`.

## GET /api/v1/recording

Choisit une interprétation avec exactement `ref` ou `name` ; un identifiant MusicBrainz convient aussi dans `name`. Aucune pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/recording' --data-urlencode 'name=Hells Bells'
```

L’objet contient `kind:"recording",reference,title,isrc,mbid,tracks,works`. `isrc` identifie un enregistrement lorsqu’il existe ; `mbid` est son identité MusicBrainz. `tracks` et `works` sont les listes complètes de références à suivre avec `/track` et `/work`. Une information externe téléchargée sans entité correspondante dans le graphe local n’est pas automatiquement transformée en enregistrement. Choix ambigu : 409 ; enregistrement absent : 404.

### HEAD /api/v1/recording

Même sélection/validation, avec statut/en-têtes sans JSON de détail.

Suite : [Genres, labels, œuvres et éditions](graph.md), [Recherche](search.md), [Inspection](inspection.md).
