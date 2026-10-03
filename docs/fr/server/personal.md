# Annotations, historique et collections personnels

Ces routes administratives exigent [l’authentification d’administration](administration.md) et visent `local`. Les [sessions de comptes](accounts.md) emploient les mêmes opérations sous `/api/me/v1`, liées à leur propriétaire authentifié. Aucune requête ne choisit un autre propriétaire. Une session `auditor` peut employer seulement les formes `GET` et `HEAD` ; toute opération personnelle `PUT`, `POST` ou `DELETE` renvoie `403 forbidden`. Catalogue sur disque et `user.json` sont lus sous verrou pour valider et publier dans une même version complète.

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

Renvoie HTTP 200 et une page de `track,at,ms_played,completed,play_count,last_played`. Dates at/last_played en secondes Unix ; durée en millisecondes. Les compteurs sont les totaux historiques par piste, pas ceux de cette page. Le journal récent est borné ; les comptes survivent à la sortie d’anciens événements. Tri par date ; dates égales distinctes, avec dernier événement reçu en premier.

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

Les playlists statiques n’ont pas encore de route native `/api/me/v1`, mais l’[adaptateur Subsonic/OpenSubsonic](subsonic.md) lit et écrit des playlists privées ordonnées dans le même fichier personnel. Il sert également les pochettes JPEG/PNG cataloguées à côté des fichiers et les fichiers audio originaux, et enregistre les scrobbles déclarés séparément des événements `Play` : aucune durée écoutée ni fin de lecture n’est inventée. Les deux types alimentent les compteurs globaux par piste ; la page native d’historique liste uniquement les événements `Play`. Une déclaration temporaire de lecture en cours, privée à son propriétaire, n’ajoute ni compteur ni historique.

Annotations de relations, review des sources, check/analyze/fingerprint, copy, backup/restore, reset, merge, inspection arbitraire de fichiers et commandes shell n’ont pas de routes HTTP actuelles. Paroles/textes restent hors des contrats HTTP. Leur existence en CLI ne les rend pas disponibles par API.
