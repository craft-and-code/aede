# Genres, labels, œuvres et éditions

Ces routes suivent les [règles communes de pages et de sélecteurs](http.md). GET renvoie HTTP 200 en cas de réussite ; HEAD valide les mêmes paramètres avec statut/en-têtes identiques, sans corps. Les relations de détail sont des tableaux complets, sans pagination. Elles viennent du catalogue local, jamais d’une recherche automatique en ligne.

## GET /api/v1/genres

Paramètres : `q` ou `name` (fragment normalisé de nom), `sort=catalog|name`, `order=asc|desc`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/genres' --data-urlencode 'q=jazz'
```

Chaque élément contient `kind:"genre",reference,name,release_count,track_count`. Les nombres décrivent les liens du graphe, pas la présence d’un tag explicite dans chaque fichier. Sans résultat, la page est vide. Deux sélecteurs textuels, un paramètre répété ou un tri non pris en charge produisent 400.

### HEAD /api/v1/genres

Même filtres sans page JSON : `curl -I 'http://127.0.0.1:8787/api/v1/genres?limit=1'`.

## GET /api/v1/genre

Exactement `ref` ou `name`, sans pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/genre' --data-urlencode 'name=Jazz'
```

Renvoie `kind:"genre",reference,name,releases,tracks`. Les liens de pistes incluent les genres hérités de l’album : une piste peut apparaître sans tag de genre propre. Suivez les références avec `/album` et `/track`. Nom ambigu : 409 avec candidats ; genre absent : 404.

### HEAD /api/v1/genre

Même sélection/statut, sans corps de détail.

## GET /api/v1/labels

Paramètres : `q` ou `name`, `mbid` exact sensible à la casse, `sort=catalog|name`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/labels' --data-urlencode 'q=Blue Note'
```

Chaque élément contient `kind:"label",reference,name,mbid,release_count` ; `mbid` peut être null. Il s’agit des labels du catalogue, pas d’une biographie ni d’une requête Discogs automatique. Sans résultat, la page est vide ; les paramètres invalides/répétés donnent 400.

### HEAD /api/v1/labels

Même validation sans corps : `curl -I 'http://127.0.0.1:8787/api/v1/labels?limit=1'`.

## GET /api/v1/label

Exactement `ref` ou `name`, sans pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/label' --data-urlencode 'name=Blue Note'
```

Renvoie `kind:"label",reference,name,mbid,releases`. Suivez les références pour consulter les éditions locales. Label absent : 404 ; label ambigu : 409 avec candidats. Cette route ne renvoie pas chaque panneau de texte affiché par la CLI.

### HEAD /api/v1/label

Valide le même choix de label sans son JSON.

## GET /api/v1/works

Une œuvre est une composition, distincte de son interprétation enregistrée. Paramètres : `q` ou `name` (fragment de titre), `mbid` exact, `sort=catalog|name|title`, `order`, `offset`, `limit`. Les tris `name` et `title` sont équivalents.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/works' --data-urlencode 'q=Concerto'
```

Chaque élément contient `kind:"work",reference,title,mbid,recording_count`. Seules les œuvres représentées dans le graphe local apparaissent. Une information externe d’œuvre ne crée pas à elle seule une œuvre locale. Une page vide ne prouve pas que le compositeur n’a jamais écrit cette œuvre. Paramètres invalides : 400.

### HEAD /api/v1/works

Même paramètres/statut sans JSON : `curl -I 'http://127.0.0.1:8787/api/v1/works?limit=1'`.

## GET /api/v1/work

Exactement `ref` ou `name` ; un identifiant MusicBrainz d’œuvre peut être fourni dans `name`. Aucune pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/work' --data-urlencode 'name=Concerto'
```

Renvoie `kind:"work",reference,title,mbid,recordings`. Le tableau relie les interprétations locales : suivez `/recording`, puis `/track` pour atteindre les fichiers. 409 indique plusieurs compositions possibles ; 404 signifie qu’aucune n’est représentée localement. La requête ne télécharge pas l’œuvre manquante.

### HEAD /api/v1/work

Même choix et erreurs, sans corps de détail.

## GET /api/v1/release-groups

Un groupe de sorties représente l’identité d’un album commune à ses éditions ; un `release` est une édition locale particulière. Paramètres : `q` ou `name`, `mbid` exact, `sort=catalog|name|title`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/release-groups' --data-urlencode 'q=Back in Black'
```

Les éléments contiennent `kind:"release_group",reference,title,mbid,release_count`. Ce nombre compte les éditions locales liées, pas toutes les éditions mondiales. Aucun résultat donne une page vide ; paramètres incompatibles/non pris en charge : 400.

### HEAD /api/v1/release-groups

Même validation avec statut/en-têtes seulement : `curl -I 'http://127.0.0.1:8787/api/v1/release-groups?limit=1'`.

## GET /api/v1/release-group

Exactement `ref` ou `name` ; un identifiant MusicBrainz convient dans `name`. Aucune pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/release-group' --data-urlencode 'name=Back in Black'
```

Renvoie `kind:"release_group",reference,title,mbid,releases`. Suivez chaque référence avec `/album` pour comparer vos éditions locales. Ambiguïté : 409 avec candidats ; absence : 404. Il ne s’agit ni d’une discographie externe complète ni d’un calcul d’albums manquants.

### HEAD /api/v1/release-group

Même résolution et statut sans JSON.

Suite : [Inspection et routes de compatibilité](inspection.md).
