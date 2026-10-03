# Compatible Aède : spécification pour les clients

Cette spécification s'adresse aux développeurs qui créent ou adaptent un lecteur, par exemple Ples. Elle précise ce qu'un client peut annoncer comme **Compatible Aède** avec les interfaces disponibles aujourd'hui. C'est un cahier des charges d'intégration, pas un programme de certification ni la promesse que toutes les fonctions de la CLI possèdent un équivalent HTTP.

**DOIT** (**MUST**) désigne une exigence du profil annoncé. **DEVRAIT** (**SHOULD**) désigne une recommandation ; documentez toute exception volontaire. **PEUT** (**MAY**) désigne une possibilité facultative. Les exigences portent sur le comportement, y compris les erreurs, sans imposer de langage, d'interface graphique ou de bibliothèque audio. Les références liées restent la source de vérité pour les champs, les limites et les codes d'erreur.

## Annoncer un profil

| Profil | Périmètre obligatoire | Sens de l'annonce |
| --- | --- | --- |
| **Aède Catalogue v1** | Règles communes, authentification native et catalogue. | Lecteur du graphe actuellement exposé par l'API native, avec actualisation du catalogue et attribution des faits. La lecture audio n'est pas annoncée. |
| **Aède Lecteur natif v1** | Toutes les exigences Catalogue v1, données personnelles natives et lecture PCM native. | Lecteur utilisant le traitement audio actuel du serveur Aède et l'historique fondé sur la consommation, avec les fonctions personnelles natives actuelles. C'est le profil natif complet défini ici. |
| **Aède Subsonic** | Règles communes applicables, authentification de l'adaptateur et opérations Subsonic annoncées par le client. | Client de l'adaptateur de compatibilité. Annoncez séparément navigation, audio original, pochettes et fonctions personnelles ; cela n'annonce ni la couverture complète de Subsonic/OpenSubsonic, ni le graphe/DSP natif. |

Un client PEUT combiner les profils, par exemple Lecteur natif v1 avec vues de pochettes et playlists statiques de l'adaptateur. Il DOIT séparer les identifiants d'accès, les identifiants d'entités, le traitement audio et les règles d'historique de chaque interface. Il n'existe pas de route publique de résolution référence native ↔ identifiant de l'adaptateur : des vues indépendantes sont possibles, sans promettre de correspondance automatique ou reconstruire les identifiants opaques depuis les détails d'implémentation. Une intégration exclusivement Subsonic doit être présentée ainsi, sans annoncer le profil Lecteur natif v1.

La documentation du client doit préciser sa version, la version d'Aède testée, la version du profil/API, les plateformes et appareils testés, les fonctions facultatives et les limites connues. Distinguez les vérifications automatiques du protocole d'une lecture réellement testée sur un appareil. L'API native v1 n'a pas de route de négociation des fonctions : `GET /api/v1/status` donne `api_version`, pas une liste de possibilités du client. Une fonction facultative indisponible doit rester indisponible dans l'interface.

Pour une intégration hypothétique de Ples, l'annonce pourrait être : « Ples version X prend en charge Aède Lecteur natif v1 avec Aède version Y sur macOS, ainsi que les pochettes et playlists statiques privées de l'adaptateur. Le déplacement temporel natif et la persistance de file serveur sont indisponibles. Les vérifications de protocole passent ; la lecture a été testée sur la sortie Z. » C'est un exemple à remplir avec les résultats réels, pas une affirmation que Ples implémente déjà ces interfaces.

## Exigences communes

| ID | Exigence |
| --- | --- |
| C1 | Le client DOIT utiliser les interfaces documentées. Il NE DOIT PAS éditer les fichiers JSON d'Aède, supprimer son verrou, écrire des tags ou remplacer les sources audio/pochettes lors de la navigation ou de la lecture. Les versions de stockage sont indépendantes des versions HTTP. |
| C2 | Le client DOIT considérer noms, tags, notes et textes externes comme non fiables : afficher leur texte littéralement, échapper le balisage si nécessaire et ne jamais les exécuter comme HTML, instructions shell ou commande d'ouverture de fichier. |
| C3 | Le client DOIT séparer les caches du catalogue par connexion serveur configurée et les caches personnels par compte authentifié. La connexion native fournit `account.id` ; un client de l'adaptateur doit distinguer les configurations compte/clé. À la déconnexion ou au changement de compte/serveur, arrêter la lecture et détacher les vues privées et données personnelles en cache. v1 n'expose pas d'UUID d'installation : réinitialiser/remplacer une configuration de connexion doit invalider son cache, sans supposer qu'une adresse représente toujours les mêmes données. |
| C4 | Le client DOIT présenter une requête échouée comme un échec. Il NE DOIT PAS la remplacer par une bibliothèque vide, inventer un fait absent ou ignorer silencieusement un réglage refusé. Réessayer les lectures avec une attente progressive et bornée ; respecter `Retry-After` lorsqu'il est fourni. Ne pas relancer aveuglément une modification après une réponse incertaine. |
| C5 | Le client DOIT exclure les secrets des diagnostics générés, exports partagés et télémétrie, et masquer les champs secrets par défaut. Les jetons Bearer natifs NE DOIVENT PAS apparaître dans les URL. L'adaptateur transmet les secrets dans les paramètres ou formulaires : masquer les requêtes complètes en conséquence. Les secrets persistants DEVRAIENT utiliser le gestionnaire de secrets du système ; les sessions natives temporaires devraient rester en mémoire. |
| C6 | L'accès distant DOIT utiliser HTTPS/WSS avec vérification normale du certificat et du nom d'hôte. Conserver l'autorité et le port configurés. Un client natif PEUT omettre `Origin` ; une origine de navigateur fournie DOIT respecter la règle documentée de même origine. Ne pas contourner les vérifications Host/Origin, se fier aux en-têtes de proxy ou désactiver TLS pour obtenir une connexion. |
| C7 | La navigation du catalogue NE DOIT PAS lancer de recherche externe de métadonnées sans fonction explicitement demandée par l'utilisateur. Faits locaux, affirmations externes et données dérivées DOIVENT rester distincts, avec l'attribution de tout contenu externe affiché. |

Voir [HTTP et JSON](http.md), [l'accès distant](remote.md), [la référence API](../api.md) et [la référence complète des routes](../../../crates/aede-server/README.md).

## Authentification native et périmètre des comptes

| ID | Exigence |
| --- | --- |
| A1 | Lorsque les comptes sont configurés, le client DOIT se connecter par `POST /api/auth/v1/session` et envoyer exactement un en-tête `Authorization: Bearer <session>` sur les requêtes natives et les ouvertures WebSocket. Les clés de l'adaptateur et l'ancien jeton d'installation ne remplacent pas une session de compte sur `/api/me/v1`. |
| A2 | Le client DOIT gérer l'expiration absolue, l'inactivité, la révocation et le redémarrage du serveur. En cas d'échec d'authentification (`401 unauthorized` ou `authentication_expired` en lecture), fermer les flux protégés, retirer la session inutilisable et demander une nouvelle authentification. Les autres erreurs, comme un réglage invalide ou des traitements occupés, ne doivent pas être confondues avec l'expiration. Une reconnexion NE DOIT PAS créer silencieusement une nouvelle lecture/écoute. |
| A3 | Les opérations personnelles DOIVENT utiliser `/api/me/v1` et le propriétaire déterminé par le serveur. Le client NE DOIT PAS fournir un autre propriétaire ou utiliser `/api/admin/v1` pour consulter les données personnelles d'un autre compte. L'administration de l'installation est une fonction locale, facultative et explicitement séparée. |
| A4 | Un `auditor` DOIT disposer d'une interface en lecture seule : catalogue et lectures de ses propres données personnelles sont permis ; audio et modifications personnelles sont indisponibles. Le client DOIT gérer un `403` du serveur même si un rôle précédemment mis en cache autorisait l'action. |
| A5 | La déconnexion DOIT révoquer la session lorsque le serveur est accessible, arrêter la lecture protégée et supprimer la session locale même en cas d'échec réseau. Un changement de mot de passe invalide les sessions ; ne plus présenter l'ancienne comme utilisable. |

Le [contrat des comptes](accounts.md) définit corps de connexion, rôles, expiration, limitation des tentatives et changement de mot de passe. Un lecteur de catalogue PEUT prendre en charge le mode anonyme sur **HTTP local en boucle locale** lorsque le serveur le permet ; il DOIT aussi fonctionner avec des comptes avant d'annoncer Catalogue v1. HTTPS exige toujours des comptes.

Le WebSocket standard d'un navigateur ne peut pas ajouter cet en-tête Bearer. Le contrat actuel de lecture native nécessite donc un transport client capable de le définir. Un site ne doit pas remplacer cet en-tête par un jeton dans l'URL ou un cookie et annoncer sa compatibilité : l'authentification/transport des navigateurs nécessite un futur contrat propre.

## Catalogue natif, identité et synchronisation

| ID | Exigence |
| --- | --- |
| G1 | Le client DOIT préserver les entités et liens exposés par v1 : une release est une édition locale, un recording une interprétation enregistrée, un track sa position dans une édition, un work une composition. Il DOIT distinguer les éditions/interprétations de même titre et suivre les liens vers œuvres, enregistrements, groupes d'éditions et artistes lorsqu'ils existent. Une hiérarchie d'albums PEUT servir de vue ; elle ne doit pas remplacer l'identité. |
| G2 | Le client DOIT copier les jetons `reference` tels qu'ils sont reçus et les encoder pour une utilisation comme paramètres d'URL. Ne pas fabriquer d'identifiants à partir des positions dans le catalogue, des noms affichés, des seuls MBID ou d'un chemin analysé. Les références appartiennent à ce catalogue ; déplacer un fichier peut les changer. Une référence absente DOIT être signalée puis actualisée, jamais réaffectée à un élément arbitraire de même nom. |
| G3 | Le client DOIT appliquer les règles documentées de pagination et de sélection, notamment la distinction entre tableaux de détails complets et listes paginées. Pour un chargement cohérent sur plusieurs pages, comparer `scanned_at` et recommencer s'il change. v1 ne garantit pas un instantané entre requêtes ; ne pas annoncer un instantané atomique catalogue/sources. |
| G4 | Le client DOIT distinguer inconnu (`null` ou statut explicite), liens absents (tableaux vides), page vide réussie et requête échouée. Une ambiguïté de nom (`409 ambiguous_entity`) exige de choisir une référence retournée, sans sélectionner silencieusement le premier résultat. |
| G5 | Le client DOIT conserver le sens des faits locaux et origines/analyses attribuées disponibles, notamment confiance, approbation, obsolescence et champs de source/version. Une analyse obsolète ne doit pas devenir une mesure actuelle ; une abréviation dérivée de pays ne doit pas devenir un code ISO officiel. Ne pas déduire un rôle de compositeur de la présence comme artiste d'album. |
| G6 | Le client DOIT ignorer les champs de réponse ajoutés et rétablir sa vue après remplacement/suppression du catalogue et reconnexion. Il DEVRAIT s'abonner à `/api/v1/events` pour actualiser rapidement ; une interrogation périodique bornée est une autre possibilité. Les événements sont des notifications sans garantie exhaustive, pas des modifications partielles du graphe ou un journal rejouable. |

Utiliser [les routes du catalogue](catalog.md), [la navigation du graphe](graph.md), [la recherche](search.md), [l'inspection](inspection.md) et [les événements](events.md). L'API native expose certaines vues du graphe, pas toutes les opérations de crédits, relations ou décisions sur les sources du cœur. Ne pas inventer de route pour les autres.

Les champs `path` et `cover_path` décrivent des fichiers du serveur ; ce ne sont ni des URL de téléchargement ni une autorisation d'accéder au système de fichiers du client. v1 n'a pas de route native d'image binaire. Un client combinant les profils PEUT utiliser les identifiants de pochette annoncés par l'adaptateur, avec son authentification distincte.

## Fonctions personnelles natives

Lecteur natif v1 DOIT fournir une vue privée des [fonctions personnelles actuelles](personal.md) du propriétaire, selon les formes et limites qui y sont définies :

| ID | Exigence |
| --- | --- |
| P1 | Favoris, notes de 1 à 5, commentaires et tags personnels DOIVENT être lus et modifiés par `/api/me/v1/annotation`. Préserver les champs absents d'une modification ; suppression explicite et valeur omise sont différentes. Une note native vaut 1–5 ou `null` pour la retirer ; `setRating` de l'adaptateur utilise 0 pour le retrait. |
| P2 | Le client DOIT présenter l'historique natif du propriétaire avec les champs de mesure/déclaration retournés et distinguer journal d'événements conservés et compteurs cumulés. Les scrobbles de l'adaptateur alimentent les compteurs mais ne sont pas des lignes `Play` natives ; une déclaration temporaire de lecture en cours n'est ni l'un ni l'autre. |
| P3 | Les collections de requêtes enregistrées DOIVENT rester des expressions évaluées pour le propriétaire actuel. Elles NE DOIVENT PAS être présentées comme des playlists statiques ordonnées. Leurs modifications/suppressions doivent utiliser les méthodes documentées et signaler l'échec sans annoncer un enregistrement réussi. |
| P4 | Le client DOIT gérer un stockage/des traitements occupés sans remplacer les données du serveur depuis un cache obsolète. Une modification optimiste de l'interface doit être réconciliée avec le résultat retourné, ou annulée/signaler une issue incertaine. v1 ne fournit pas de jeton de fusion/révision pour des éditeurs concurrents. |

v1 n'expose pas de persistance native de playlists statiques. Un client combinant les profils PEUT les ajouter via l'adaptateur ; répétitions et ordre doivent survivre à un aller-retour. Une piste manquante ne doit pas disparaître silencieusement d'une playlist enregistrée. Les annotations de relations et décisions sur les sources restent réservées à la CLI.

## Lecture PCM native et historique exact

Le [contrat de lecture PCM v1](playback.md) définit les messages et délais. Lecteur natif v1 DOIT implémenter le cycle complet start → format → audio → EOF → recorded/fermeture, au-delà de la simple réception d'octets WebSocket.

| ID | Exigence |
| --- | --- |
| L1 | Ouvrir `/api/me/v1/playback` avec la session, puis démarrer exactement une piste actuellement cataloguée. Valider et afficher normalisation, fréquence et tonalité demandées. Attendre `format` avant d'interpréter l'audio ; il décrit le format transmis, qui peut différer du fichier source. |
| L2 | Lire les messages binaires comme des frames complètes `f32le` entrelacées, selon les canaux/fréquence annoncés. Conserver ordre et alignement des canaux. Compter les **frames**, pas les octets ou les échantillons individuels d'un canal ; les limites de messages ne sont pas des limites musicales. |
| L3 | Maintenir une file audio bornée et n'envoyer des `ack.frames` cumulés non décroissants que pour les frames effectivement consommées par la sortie, sans dépasser le total reçu. Réception réseau ou mise en file du périphérique ne signifie pas consommation. Les frames abandonnées/sautées NE DOIVENT PAS être acquittées. Si la conversion du périphérique change la fréquence, ramener la consommation aux frames transmises plutôt qu'envoyer le compteur du périphérique. Répéter un acquittement identique est permis mais ne constitue pas un progrès. |
| L4 | Respecter fenêtre d'acquittement et délai de consommation, y compris en pause, en cas de panne de périphérique ou de sortie lente. Répéter un acquittement/envoyer un ping ne constitue pas un progrès. Une pause longue peut terminer ce socket ; reprise/reconnexion doit être explicite, sans prétendre qu'un déplacement temporel est disponible. |
| L5 | À EOF, consommer le PCM restant, y compris la fin du convertisseur, acquitter le total final puis attendre `recorded` avant d'annoncer l'historique enregistré. Une fermeture/erreur peut produire une écoute incomplète ; révocation ou source modifiée peut empêcher l'enregistrement. Une confirmation incertaine n'autorise pas à soumettre la même écoute une seconde fois. |
| L6 | Le client NE DOIT PAS ajouter un POST d'historique natif ou un scrobble de l'adaptateur pour cette même écoute native. Aède enregistre déjà au plus un `Play` acquitté par socket. Les acquittements déclarent une consommation, sans prouver qu'une personne a entendu le son. |
| L7 | Le client DOIT rendre explicites normalisation/égalisation/rééchantillonnage supplémentaires et éviter d'appliquer deux fois le même traitement choisi. Le PCM serveur est déjà passé par le DSP partagé, la réduction des canaux et la protection de sortie sélectionnés. Il NE DOIT PAS être présenté comme le fichier encodé original, une sortie bit-perfect ou un plafond true peak physique garanti. |
| L8 | Arrêt, saut de piste, perte de connexion, déconnexion du compte et panne du périphérique DOIVENT arrêter la lecture correspondante et abandonner le PCM non consommé. Une piste utilise un socket ; une file locale au client PEUT choisir les pistes suivantes, sans annoncer persistance de file serveur, déplacement temporel ou enchaînement gapless garanti par v1. |

Le client gère son périphérique de sortie et son horloge. Il DEVRAIT acquitter par petits lots réguliers avec une marge suffisante avant le délai documenté, borner le travail audio et mesurer coupures, latence et consommation de frames sur le matériel visé. Des tests de protocole seuls ne démontrent pas la qualité d'écoute ou des enchaînements sans interruption sur un appareil.

Par exemple, 38 400 octets de `f32le` stéréo à 48 000 Hz correspondent à `38400 / (2 × 4) = 4800` frames, soit 100 ms. Après consommation de ce premier lot, envoyer `{"type":"ack","frames":4800}` ; après consommation d'un deuxième lot identique, envoyer 9600, pas 4800. Calculer l'écoute avec la fréquence du format transmis, pas la durée du catalogue ou la fréquence éventuellement différente du périphérique. Les [régressions de lecture du serveur](../../../crates/aede-server/src/playback_api_tests.rs) couvrent EOF complet, écoutes incomplètes, compteurs invalides, révocation, sources modifiées et fin du convertisseur ; elles servent de référence d'intégration, sans remplacer les tests du client.

## Profil de l'adaptateur Subsonic/OpenSubsonic

Suivre [le guide de l'adaptateur](subsonic.md) pour méthodes actuelles, limites, identifiants et erreurs. Ne pas déduire une prise en charge du seul nom du protocole ou de `getLicense`.

| ID | Exigence |
| --- | --- |
| S1 | Utiliser une clé d'application révocable propre au client. Préférer `apiKey` si possible ; l'ancien transport `u+p` n'est permis qu'avec la **clé d'application complète** dans `p` et le nom de compte correspondant dans `u`. Ne pas envoyer le mot de passe natif ou transformer la clé en jeton MD5 non pris en charge. `enc:` est un encodage, pas un chiffrement. |
| S2 | Le client DEVRAIT lire les extensions publiques lorsque sa bibliothèque de protocole le permet ; le transport legacy documenté reste valide sans cet appel. Il DOIT n'utiliser que les méthodes/réglages admis, envoyer version du protocole et nom du client, et vérifier `subsonic-response.status` et les erreurs XML/JSON même si HTTP vaut 200. Préserver les paramètres de liste répétables et ne jamais dupliquer les secrets/options scalaires. |
| S3 | Séparer les identifiants opaques de l'adaptateur des références natives et les actualiser après une entité absente. Ne pas déduire une identité commune entre interfaces du seul nom affiché. La projection albums/pistes/artistes ne contient pas tous les liens, analyses et champs de source du graphe natif. |
| S4 | Demander l'audio original avec les options admises. Le client gère décodage, conversion du périphérique et DSP facultatif ; le serveur transfère les octets originaux sans traitement PCM natif. Le déplacement par plage d'octets est une opération du client sur le fichier encodé, pas un seek PCM natif. Codec absent ou conversion refusée DOIT être visible. |
| S5 | Si les pochettes sont prises en charge, utiliser un identifiant annoncé. Demander une vignette ne change que la réponse ; cela NE DOIT PAS faire croire que la pochette téléchargée a été réduite ou réécrite. Le client PEUT mettre en cache des vignettes privées dans le périmètre compte/serveur. |
| S6 | Si les fonctions personnelles sont annoncées, conserver lors des échanges favoris/notes et playlists privées ordonnées du propriétaire, sans perdre les répétitions ou sélectionner un autre propriétaire. Les auditeurs peuvent consulter métadonnées/pochettes, mais ni écouter/télécharger ni modifier leurs données. |
| S7 | Un scrobble soumis DOIT rester une déclaration piste/date avec durée écoutée et complétude inconnues. Le flux audio seul n'enregistre pas d'écoute. Ne pas inventer des millisecondes écoutées depuis la durée du catalogue ou transformer la lecture en cours en historique. Les soumissions ne sont pas idempotentes : une nouvelle tentative incertaine ne doit pas compter deux fois silencieusement. |
| S8 | Tris, files, transcodages et panneaux facultatifs non pris en charge DOIVENT être indisponibles ou afficher leur indisponibilité. Configurer un tri d'album admis ; le `created` approché à partir du scan n'est pas une date d'ajout d'album et ne permet pas `newest`. |

Un lecteur de l'adaptateur PEUT normaliser localement si c'est clairement expliqué ; il ne doit pas annoncer l'utilisation du DSP natif du serveur. Un lecteur hybride doit choisir un seul circuit d'historique par écoute et ne pas transformer un scrobble sans mesure en `Play` natif à durée inventée.

## Matrice d'acceptation

Exécuter ces vérifications sur serveur/dossier de données isolés et comptes jetables ; ne jamais endommager/réécrire la musique de l'utilisateur pour tester des erreurs. Les vérifications **obligatoires** correspondent au profil annoncé et aux fonctions facultatives prises en charge. Conserver les résultats avec versions client/serveur, plateforme et date.

| Vérification | Profils | Cas d'acceptation reproductible |
| --- | --- | --- |
| Authentification et périmètre | Tous les profils authentifiés ; cas personnels/audio si implémentés | Utiliser A, B et un auditeur et vérifier les lectures du catalogue permises. Si les fonctions personnelles/audio existent, favoris/commentaires/historique de A ne doivent pas apparaître dans la vue privée de B ; une sélection de propriétaire/modification refusée ne sauvegarde rien, et la lecture auditeur est indisponible/refusée. |
| Expiration et révocation | Tous les profils authentifiés | Révoquer session/clé pendant lecture ou consultation, puis redémarrer le serveur. L'accès protégé cesse ; les sessions natives exigent une connexion, les clés non révoquées survivent à un redémarrage ordinaire. |
| Transport et secrets | Tous | Vérifier HTTPS valide, autorité incorrecte, certificat non approuvé et Origin étrangère. Refus visibles, aucun contournement de vérification. Vérifier que journaux/rapports masquent les secrets. |
| Identité et graphe | Natifs | Utiliser deux éditions de même titre et un enregistrement lié à plusieurs pistes ; suivre liens enregistrement/œuvre/groupe d'éditions et préserver les identités. Déplacer un fichier de test, rescanner et signaler l'ancienne référence sans réaffectation arbitraire. |
| Pages et changements | Natifs | Charger plusieurs pages et une page vide après la fin. Remplacer/supprimer/rescanner entre deux pages et reconnecter les notifications ; reprendre un chargement incohérent et distinguer catalogue absent et vide. |
| Attribution et faits inconnus | Natifs | Afficher séparément origine inconnue, origine enregistrée attribuée et analyse importée actuelle/obsolète. L'absence reste inconnue ; sa consultation ne lance pas de recherche externe. |
| Entrées, ambiguïté et saturation | Tous | Tester noms ambigus, réglage non admis, référence malformée, stockage/traitements occupés et réponse interrompue. Afficher les codes stables ; les reprises bornées de lectures ne créent pas de boucle de modifications. |
| Aller-retour personnel | Lecteur natif ; option personnelle adaptateur | Modifier un champ sans vider les autres, retirer une note, recharger et changer de compte. Distinguer collections de requêtes et playlists ordonnées avec doublons si implémentées. |
| Frames PCM et compteur de sortie | Lecteur natif | Lire fixtures mono/stéréo avec fréquences source/périphérique différentes ; vérifier frames complètes, correspondance consommation/transmission et aucun acquittement à la seule réception/mise en file. |
| Historique PCM complet/partiel | Lecteur natif | Consommer toutes les frames EOF et vérifier un événement complet avec millisecondes acquittées. Arrêter tôt : événement incomplet ; ne rien consommer : aucun événement. Aucun historique/scrobble supplémentaire. |
| PCM bloqué et erreurs | Lecteur natif | Suspendre la consommation, déconnecter, provoquer panne du périphérique, révoquer la session et changer une source de test isolée. Respecter fermetures/erreurs, abandonner l'audio en attente et ne pas annoncer une écoute complète non confirmée. |
| Traitement et lecture physique | Lecteur natif | Comparer off/track/album et tonalité, gestion canaux/fréquences non admis et conversion du périphérique. Mesurer continuité/coupures réelles sur le matériel annoncé ; préciser les qualités non mesurées. |
| Audio original et pochettes | Options médias adaptateur | Comparer empreintes de fichiers originaux, réponses complètes/plage unique/plage impossible et demande de vignette. Les empreintes des sources audio/pochettes restent identiques. |
| Déclarations de l'adaptateur | Option scrobble/lecture en cours | Vérifier compteur/journal sans durée inventée, aucun compteur pour lecture en cours, expiration et isolation. Simuler une réponse perdue et conserver l'issue explicitement incertaine. |
| Compatibilité additive | Tous | Ajouter champs de réponse/types d'événement inconnus à une fixture et continuer à traiter les champs obligatoires. Refuser les demandes non admises dans l'interface sans inventer un succès de substitution. |

Automatiser analyse des messages, identité, isolation des comptes, erreurs et calcul des frames. Compléter par des vérifications d'interface et une lecture réelle sur chaque sortie prise en charge. Les tests du serveur établissent son comportement ; ils ne certifient pas l'implémentation d'un client tiers.

Avant d'annoncer une compatibilité complète **Aède Lecteur natif v1**, toutes les exigences DOIT de C, A, G, P et L doivent être couvertes par les cas d'acceptation correspondants réussis. Un cas obligatoire non vérifié limite l'annonce à une intégration provisoire, avec ce cas explicitement listé. Pour Catalogue v1, appliquer C/A/G ; pour Aède Subsonic, appliquer les règles C/S pertinentes et préciser méthodes/fonctions admises et état de leur validation.

Utiliser cette liste avant publication, avec les résultats détaillés :

- [ ] Annoncer profil, versions client/serveur et interfaces facultatives.
- [ ] Relier chaque exigence DOIT applicable à une vérification réussie ; qualifier de provisoire toute annonce non vérifiée.
- [ ] Vérifier isolation des propriétaires, révocation, transport et masquage des secrets.
- [ ] Vérifier actualisation, identités obsolètes, faits inconnus et erreurs visibles.
- [ ] Vérifier le circuit audio/historique choisi sans double traitement ni double écoute.
- [ ] Consigner appareils/plateformes réellement testés et limites restantes.

## Futurs contrats et limites de l'annonce

Déplacement temporel natif distant, enchaînements continus, file/bookmarks persistants, playlists statiques, pochettes binaires, paroles/textes externes, exploration complète crédits/relations et authentification navigateur par cookie/WebSocket n'ont pas encore de contrat natif v1. Ce ne sont pas des exigences cachées du profil actuel Lecteur natif v1 ; les fonctions CLI existantes n'autorisent pas l'invention de routes HTTP. Certaines possibilités, notamment pochettes/playlists statiques/téléchargements originaux, disposent déjà du contrat distinct de l'adaptateur.

Une future interface native doit définir sens, permissions, erreurs, tests et compatibilité avant d'être annoncée. Ajouter une route/un champ v1 ne l'ajoute pas immédiatement aux exigences obligatoires du profil ; élargir ce périmètre demande une révision de cette spécification. Une rupture HTTP relève d'une nouvelle version de l'API. Suivre [la feuille de route](../../design/roadmap.md) et [l'état actuel](../../coding/current-state.md), en prenant les contrats publiés liés comme cible d'implémentation.
