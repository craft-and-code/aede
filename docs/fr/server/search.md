# Recherche et query par HTTP

Search trouve des noms dans le graphe. Query sélectionne des **pistes** avec une expression. Ces lectures publiques ne téléchargent jamais de données manquantes. Les paramètres doivent être encodés ; les résultats suivent la [pagination habituelle](http.md). Le texte est limité à 2048 octets ; une expression publique accepte au plus 64 unités de complexité (termes, parenthèses et négations).

## GET /api/v1/search

`q` obligatoire ; `comments=true|false` facultatif (false par défaut), `offset`, `limit`. Aucun paramètre de tri.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/search' \
  --data-urlencode 'q=Miles Davis' --data-urlencode 'comments=false'
```

Renvoie HTTP 200 et une page classée par pertinence. Chaque résultat contient `kind,reference,name,context,found_in`. `kind` indique le type d’entité ; `context` apporte une explication lisible. `found_in:"name"` signale un nom. Avec `comments=true`, les **commentaires de tags de fichiers** correspondants sont ajoutés avec `found_in:"comment"` ; ce ne sont pas vos notes Markdown personnelles. Une piste peut apparaître deux fois si son nom et son commentaire correspondent. Un client doit gérer une référence null plutôt que l’assimiler à un indice.

Un `q` vide/trop long, un paramètre inconnu/répété ou une valeur comments autre que true/false produit `400 invalid_query`. Aucun résultat donne une page vide. `429 inspection_busy` invite à attendre la fin d’autres inspections.

### HEAD /api/v1/search

Même query obligatoire et mêmes validation/recherche, avec statut/en-têtes seulement : `curl -I 'http://127.0.0.1:8787/api/v1/search?q=Miles'`.

## GET /api/v1/query

`q` obligatoire (expression Aède) ; `sort`, `offset`, `limit` facultatifs. Aucun paramètre `order` séparé : un `-` à la fin du tri inverse l’ordre.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/query' \
  --data-urlencode 'q=codec:flac year:>=1980' --data-urlencode 'sort=year-'
```

Renvoie HTTP 200 et des résumés de pistes `reference,title,release,recording,duration_ms`. Les espaces signifient ET, `|` signifie OU, un `-` initial exclut une condition et les parenthèses groupent les conditions. Encadrez les valeurs contenant des espaces dans l’expression, par exemple `artist:"Miles Davis"`. Le [guide query](../../querying.md) détaille les valeurs publiques et les prédicats de relations.

Les champs publics couvrent titre, artiste, album, enregistrement, œuvre/mouvement, groupe de sorties, artiste d’album, genre, label, commentaire/chemin/codec/année/durée/taille/débit/fréquence de fichier, lossless/compilation et crédits/relations du catalogue. Notes personnelles, étoiles, favoris, tags utilisateurs et historique sont **refusés**, comme les paroles. Une expression valide dans votre CLI n’est donc pas nécessairement publique. Les collections authentifiées peuvent enregistrer des expressions personnelles ; cela n’ajoute pas de route publique d’évaluation de collection.

Tris autorisés : `catalog,title,artist,album,year,duration,length,size`, éventuellement suivis de `-` ; `length` est un alias de durée. Les tris rating/played sont refusés. Le tri précède la pagination. Même une expression correctement écrite peut produire `400 invalid_query` si elle nomme une valeur inconnue du catalogue : Aède n’ignore pas silencieusement le terme. Syntaxe cassée, complexité excessive, clauses privées/paroles et options non prises en charge donnent aussi 400. Sources illisibles : 500 ; saturation : 429.

### HEAD /api/v1/query

Valide/évalue la même expression sans page de pistes : `curl -I 'http://127.0.0.1:8787/api/v1/query?q=codec%3Aflac'`.

Les [événements WebSocket](events.md) permettent de rafraîchir les résultats après un changement de catalogue.
