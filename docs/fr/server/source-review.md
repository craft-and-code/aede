# Valider les correspondances MusicBrainz

Cette opération administrateur utilise les mêmes décisions persistantes que `aede review`. Elle concerne le catalogue commun et ne modifie ni les fichiers audio, ni leurs tags, ni les annotations personnelles. La capacité `source_review` annonce les routes lorsque l’administration est configurée. Les règles d’authentification HTTP/HTTPS et d’origine existantes s’appliquent ; utilisateurs ordinaires, auditeurs et invités n’ont pas accès à ces opérations.

## GET /api/admin/v1/source-review

Consulte les propositions MusicBrainz stockées et rattachées au catalogue courant, sans requête externe. Paramètres : `offset`, `limit` (bornes habituelles du catalogue), `q` (recherche normalisée dans le nom local, 1024 octets maximum) et `all` (booléen, false par défaut). Les paramètres inconnus sont refusés. `all=true` inclut les décisions déjà acceptées/refusées. Les propositions orphelines et autres fournisseurs restent conservés mais ne figurent pas ici.

La réponse contient `items,total,offset,limit`. Chaque élément expose :

- `id` : sélecteur complet, sans abréviation ; `revision` : empreinte opaque de concurrence, pas un secret.
- `reference,name,kind` : identité locale ; `local_mbid` : identifiant local dans le même espace MusicBrainz que la proposition, si disponible (artiste, édition ou groupe de parutions, enregistrement, label). Il participe aussi à la révision, même pour une proposition approximative.
- `source_id,url` : proposition MusicBrainz et lien HTTPS officiel, ou null si le lien ne peut pas être construit de manière sûre.
- `score` : score original de recherche, ou null ; `decision` : null, `accepted` ou `rejected`.
- `details` : paires `[champ,valeur]` bornées, par exemple type, région, pays, dates et précision. Les faits externes restent distincts du nom local.

## PATCH /api/admin/v1/source-review

Aucun paramètre d’URL. Le petit objet JSON contient exactement `id`, `revision` et `decision` (`accept`, `reject` ou `undo`). Les sélecteurs/empreintes comportent 16 caractères hexadécimaux complets. Sous le verrou d’écriture commun, Aède revérifie la session administrateur, relit le catalogue et les sources sur disque, puis compare la révision avant d’appliquer les règles existantes du cœur. Les décisions sans fiche source actuelle sont conservées.

Une proposition modifiée renvoie `409 review_changed` : actualisez la fiche, sans rejouer automatiquement le consentement. Un autre écrivain donne `409 store_busy`. Un stockage illisible refuse l’action sans remplacer les données. La réussite renvoie l’élément actualisé et publie une nouvelle révision du catalogue. `undo` retire la décision, pas la proposition. Une annulation côté client ne revient pas sur une écriture déjà validée.

## Récupérer ensuite la biographie

L’enregistrement d’une décision et la récupération en ligne sont indépendants. La capacité `fetch_artist_summary_links` autorise `POST /api/admin/v1/fetch` avec, par exemple, `{"summaries":true,"artist":"artist:example ensemble","full":true,"yes":true,"lang":"fr"}`. `artist` est une référence locale exacte, au plus 4096 octets sans NUL, exclusive de `targets` et de toute autre passe. Équivalent CLI : `fetch --summaries --artist-ref=<référence>`. Une référence absente ou ambiguë échoue sans élargir la sélection par nom.

Si une proposition abrégée déjà validée n’a pas de lien Wikidata, Aède consulte les relations détaillées de cette identité MusicBrainz. L’identifiant validé, le score original, la discographie et la décision humaine sont conservés. Chaque réponse réussie est sauvegardée immédiatement ; `relationships_complete` distingue une absence confirmée d’une ancienne fiche incomplète. Une réponse invalide ou échouée reste réessayable. Les limites du fournisseur et les règles existantes des tâches/annulations s’appliquent. MusicBrainz peut réellement ne pas fournir de lien Wikidata ou d’article utilisable ; un résultat Wikipédia par nom ne devient jamais une identité automatiquement.

La réponse détaillée doit contenir un UUID MusicBrainz valide avant d’enregistrer ses données ou son indicateur de complétude. Une fusion peut renvoyer un UUID différent de celui demandé ; un identifiant malformé ne prouve pas une absence de liens et laisse la fiche enregistrée intacte et réessayable.

Le client capture la source et la session exacte entre validation et lancement. Un changement de compte ou la destruction de la vue empêche un lancement ultérieur. Un lancement échoué conserve la décision déjà enregistrée. Une tâche acceptée appartient au registre existant, continue après navigation et conserve ses réponses terminées en cas d’[annulation](jobs.md).

Une référence valide `artist:mbid:<UUID>` est résolue vers la clé de l’artiste dans le catalogue avant de limiter la récupération. Le cache Wikipédia évite une nouvelle demande uniquement pour la même identité Wikidata. Si une identité MusicBrainz désormais validée fournit un lien Wikidata différent, l’ancien article reste conservé mais n’est plus présenté comme sa biographie. Une proposition non validée ne peut pas invalider des données indépendantes ; une décision explicite sur la fiche Wikipédia garde sa priorité. Une interruption ou un échec conserve les liens déjà récupérés et l’ancien article sans afficher une identité connue comme différente.

La même règle s’applique aux biographies de labels Discogs : un profil récent portant un autre identifiant Discogs que celui désormais validé ne bloque pas sa récupération et ne prouve pas l’identité de l’ancien profil. L’âge du cache ne suffit pas. Un profil refusé pour le même identifiant n’est pas téléchargé à nouveau du seul fait de ce refus.

## Langue des biographies et réponses conservées

Un client demandant les résumés transmet la langue de son interface dans `lang`, qui prime sur la langue du processus Aède. La passe privilégie cette langue, puis l’anglais lorsqu’aucun lien vers un article dans la langue préférée n’est présent. Elle ne traduit pas les textes sources ; chaque texte conserve sa langue réelle, son URL et sa licence.

Les faits Wikipédia d’un artiste ou d’un label peuvent conserver `summary_languages`, la préférence ordonnée d’une recherche terminée, y compris lorsqu’elle aboutit à un repli ou à une absence réelle. Un ancien texte déjà dans la langue préférée reste réutilisable sans requête. Un texte dans une autre langue, ou une ancienne absence sans ce contexte, ne bloque plus un enrichissement explicite dans la langue préférée. Un repli ou une absence confirmé est réutilisé pour la même préférence ; `full` permet toujours de réessayer explicitement. Le champ est facultatif et compatible avec les anciens magasins de sources. Un contexte présent mais illisible est refusé sans réécriture du magasin.

Une erreur réseau, une réponse malformée ou un résumé renvoyé dans une langue différente de celle de l’article demandé conserve la biographie précédente et permet une nouvelle tentative. Les rejets explicites restent applicables. Les biographies de labels privilégient le texte Wikipédia de confiance ; le profil Discogs reste conservé comme repli dans sa langue d’origine.

La navigation et le changement de langue du client ne déclenchent aucune requête fournisseur. Le texte conservé reste visible avec sa langue réelle jusqu’à la fin d’un enrichissement explicite et de la synchronisation. La couche commune conserve une seule réponse Wikipédia courante par entité, et non un magasin de traductions simultanées par client.
