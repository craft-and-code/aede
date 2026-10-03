# Comprendre HTTP et JSON

Une API est une interface qu’un programme peut appeler. L’API locale d’Aède expose des données, pas un lecteur déjà dessiné. Les exemples supposent un serveur sur `http://127.0.0.1:8787` ; adaptez le port à l’adresse affichée au démarrage.

## Lire une adresse

`http://127.0.0.1:8787/api/v1/albums?limit=10` contient l’adresse de base, une route (`/api/v1/albums`) et un paramètre (`limit=10`). Plusieurs paramètres se séparent par `&`. Pour les espaces, accents, barres obliques et références, utilisez `curl --get --data-urlencode` :

```sh
curl --get 'http://127.0.0.1:8787/api/v1/album' \
  --data-urlencode 'name=Back in Black'
```

`GET` consulte les données. `HEAD` renvoie statut/en-têtes sans corps ; `curl -I URL` effectue un HEAD. Le catalogue accepte les deux. `POST` soumet une opération, `PUT` met à jour, `PATCH` modifie certains champs de compte et `DELETE` supprime/révoque. Les mutations existent seulement sur les routes administratives, d’authentification et personnelles documentées. Les options CLI `--json`, `--output` ou `--csv` ne sont pas des paramètres HTTP.

## Lire une page de résultats

Les listes possèdent cette enveloppe ; l’exemple illustre une page vide :

```json
{"items":[],"total":0,"offset":0,"limit":50,"scanned_at":0}
```

`items` contient la page actuelle. `total` compte tous les résultats avant découpage. `offset` indique le nombre de résultats sautés, à partir de 0. `limit` est la taille maximale demandée : 50 par défaut, entre 1 et 200. La deuxième page de 50 résultats utilise `offset=50&limit=50`. Une page au-delà de la fin est une réussite avec une liste vide. Les filtres s’appliquent avant le tri, puis vient la pagination.

`scanned_at` donne la date du scan en secondes Unix. Comparez-la entre pages ; si elle change, recommencez la pagination lorsqu’une vue cohérente est nécessaire. Il n’existe pas d’instantané commun à plusieurs requêtes. Les durées en `_ms` sont en millisecondes et les tailles en octets. `null` signifie qu’un fait est indisponible ; un tableau vide indique l’absence de relations listées. Les noms conservent l’écriture locale. Un client doit ignorer les champs de réponse inconnus pour rester compatible avec les ajouts futurs.

## Choisir une entité

Une **entité** est un artiste, album, piste, enregistrement, œuvre, groupe de sorties, label ou genre. L’album est un `release` ; la piste place un `recording` dans cet album. Les réponses relient les entités avec un jeton stable `reference`. Copiez-le tel quel et transmettez-le par `--data-urlencode` ; n’inventez pas d’indices de tableau. Une référence de piste dépend du chemin et change lorsque le fichier est déplacé.

Les routes au singulier acceptent exactement l’un de `ref` ou `name`. La comparaison des noms ignore casse et accents ; une correspondance exacte normalisée passe avant une correspondance partielle. Les alias d’artistes sont reconnus. Pour les albums, enregistrements, œuvres et groupes de sorties, un identifiant MusicBrainz peut aussi être fourni dans `name`.

Si plusieurs entités correspondent, Aède renvoie `409 ambiguous_entity`, avec au plus 200 candidats `{reference,name}` dans `error.candidates`. Répétez la requête avec le `ref` souhaité. Sans résultat, la réponse est `404 entity_not_found`. Fournir les deux sélecteurs, aucun sélecteur, une référence du mauvais type ou des paramètres non pris en charge produit une erreur.

## Filtres et ordre

Les filtres se combinent avec ET. Les listes ordinaires utilisent `sort=catalog&order=asc` par défaut : l’ordre déterministe enregistré lors du scan. Chaque route précise les autres tris acceptés. Les égalités de texte conservent l’ordre du catalogue. Les années inconnues d’albums restent à la fin dans les deux sens ; les années égales se départagent par titre normalisé, puis ordre du catalogue. Les filtres exacts `mbid` distinguent la casse. Pays et années ont un ordre fixe et refusent un autre tri.

Les paramètres inconnus, répétés ou incompatibles sont refusés. `/status` et `/library` sont des exceptions historiques qui n’interprètent pas les paramètres ; n’en ajoutez pas plutôt que de dépendre de cette exception.

## Comprendre une erreur

```json
{"error":{"code":"entity_not_found","message":"no entity matches this reference"}}
```

Le statut HTTP décrit la catégorie de résultat. `error.code` est la valeur stable destinée aux logiciels ; le texte humain `message` peut changer.

| Statut / code | Réaction utile |
| --- | --- |
| 400 `invalid_query`, `invalid_parameters`, `invalid_reference` | Vérifiez noms des paramètres, sélecteur et types des valeurs. |
| 400 `invalid_pagination` | Utilisez des entiers sans signe et une limite de 1 à 200. |
| 403 `invalid_host` / `invalid_origin` | Appelez l’adresse/port locaux réels, sans origine de navigateur étrangère. |
| 404 `entity_not_found` / `not_found` | Actualisez la référence ou vérifiez l’adresse. |
| 405 `method_not_allowed` | Cette méthode HTTP n’existe pas sur cette route. |
| 409 `ambiguous_entity` | Choisissez une référence parmi les candidats. |
| 409 `store_busy` | Attendez la fin de l’écriture et réessayez la lecture ; ne supprimez pas le verrou. |
| 429 `inspection_busy` | Les deux travailleurs de navigation/inspection sont occupés ; réessayez plus tard. |
| 500 `sources_unavailable`, `catalog_read_failed`, `inspection_failed`, `store_error` | Consultez le journal serveur et préservez les données avant réparation. |
| 503 `catalog_unavailable` | Le catalogue est absent ; restaurez-le ou effectuez un scan, puis réessayez. |

Les erreurs d’authentification, de corps JSON et de tâches sont expliquées dans [Administration](administration.md), [Tâches](jobs.md) et [Données personnelles](personal.md). Les erreurs de transport/analyse HTTP et de négociation WebSocket ne garantissent pas cette enveloppe JSON.

## La frontière locale

Sans [comptes](accounts.md), les lectures du catalogue ne demandent aucun jeton et peuvent révéler chemins, commentaires et noms aux autres processus/utilisateurs. Avec des comptes, elles exigent une session Bearer ou le jeton administratif. Le serveur valide Host et Origin contre son adresse/port local ; aucune autorisation entre origines, écoute publique ou TLS.

Aucun secret dans une page web ou URL. Le jeton administratif historique refuse toute Origin ; les sessions suivent la vérification d’origine locale commune. Un futur lecteur/site nécessite encore une conception de connexion/cookies et de transport distant chiffré.

## Choisir les routes

| Besoin | Suite |
| --- | --- |
| Albums, artistes, origine, pistes et interprétations | [Catalogue](catalog.md). |
| Genres, labels, compositions et groupes d’éditions | [Graphe](graph.md). |
| Disponibilité, totaux, diagnostics, racines, rôles, pays, années et compatibilité historique | [Inspection](inspection.md). |
| Noms par pertinence et expressions publiques de pistes | [Recherche/query](search.md). |
| Notifications de catalogue et phases des tâches | [WebSocket](events.md). |
| Activer le jeton facultatif | [Administration](administration.md). |
| Soumettre/suivre/annuler scan ou fetch | [Tâches HTTP](jobs.md). |
| Annotations, historique et collections du propriétaire | [Données personnelles](personal.md). |

Le [contrat versionné de l’API](../../api.md) définit la compatibilité et les types précis ; ces guides expliquent l’utilisation de l’interface actuellement implémentée.
