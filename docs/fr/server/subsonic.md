# Clients Subsonic et OpenSubsonic

Aède propose un adaptateur de compatibilité sous `/rest/<method>` et `/rest/<method>.view`, avec le catalogue, les comptes et les données personnelles existants. Il suit l’[API Subsonic](https://www.subsonic.org/pages/api.jsp) et l’[extension OpenSubsonic de clés API](https://opensubsonic.netlify.app/docs/extensions/apikeyauth/). Les opérations disponibles pour la navigation, l’audio original, les pochettes externes et les données personnelles privées sont listées ci-dessous. L’authentification locale, la navigation par artistes/albums, la lecture du FLAC original, les notes et les favoris artistes/albums ont été confirmées avec Submariner 3.4 sur macOS. Les requêtes de Supersonic ont été étudiées dans ses sources officielles ; sa lecture réelle et la compatibilité élargie des clients restent à valider.

Dans la référence des routes, `/rest/{method}` signifie remplacer `{method}` par une méthode disponible, éventuellement suivie de `.view`.

## Créer une clé client

**Créez une clé d’application avant de configurer un client Subsonic/OpenSubsonic. Le mot de passe du compte Aède ne permet pas de se connecter à cet adaptateur.** Initialisez les [comptes](../cli/accounts.md), puis créez une clé distincte par client avec le dossier de données du serveur :

```sh
aede accounts keys alice create "Téléphone"
```

Remplacez `alice` par votre compte existant. Les commandes de ce guide utilisent `aede` lorsqu’il est dans votre `PATH`. Depuis la racine du dépôt, utilisez `./target/release/aede` ; depuis le dossier contenant un dossier `aede` extrait d’une version téléchargée, utilisez `./aede/aede` (ou `./aede` une fois à l’intérieur de ce dossier). Avec un dossier de données personnalisé, ajoutez le même `--data "/chemin/vers/donnees"` aux commandes de comptes et à `serve`.

Copiez uniquement la valeur complète après `API key (shown once):` : `id.secret`, **129 caractères avec le point** (64 caractères hexadécimaux, un point, puis 64 caractères hexadécimaux). L’ID du tableau ne permet pas de s’authentifier seul. Le secret n’est révélé qu’une fois, après sauvegarde réussie ; la liste des clés ne permet pas de le récupérer. Conservez la clé complète dans un emplacement privé, ou créez une remplaçante si vous l’avez perdue. Les [étapes Submariner ci-dessous](#configuration-locale-de-submariner-sur-macos) indiquent exactement où la coller.

Création et révocation sont prises en compte sans redémarrer le serveur. Les clés n’expirent pas automatiquement, survivent au redémarrage et se révoquent séparément. Voir [liste et révocation des clés](../cli/accounts.md). La liste expose seulement `id,label,created_at` ; avec `--json`, la création ajoute `token` à ces métadonnées, tandis que liste/révocation produisent un tableau de métadonnées. Changer mot de passe/nom/rôle/état, exécuter `accounts revoke` ou restaurer les identifiants invalide les clés concernées ; restaurer les comptes change la génération globale et retire toutes les clés restaurées. Une ancienne sauvegarde sans comptes conserve les clés courantes. Recréez-les après récupération des identifiants. Maximum : huit clés par compte, 512 par installation ; libellés non blancs de 128 octets UTF-8 au plus.

Le store privé conserve uniquement des vérificateurs Argon2 salés pour les secrets. Un client peut fournir `apiKey=<id.secret>`, ou utiliser les anciens champs `u=<nom-du-compte>&p=<clé-id.secret-complète>` s’il ne propose pas de champ de clé API. Le champ mot de passe contient la clé client révocable, pas le mot de passe du compte. L’encodage hexadécimal de la clé complète sous `p=enc:<hex>` est aussi accepté ; il ne chiffre pas la requête. Le nom d’utilisateur doit correspondre au compte actuel de la clé. La connexion par mot de passe du compte reste indisponible (code 42), comme le MD5 salé `u+t+s` (41). Combiner `apiKey` avec un ancien paramètre d’identification renvoie 43 ; une clé invalide/révoquée ou un autre compte renvoie 44. Les sessions Bearer natives restent propres à `/api` : elles n’authentifient pas cet adaptateur, et les clés ne permettent pas d’accéder aux routes natives.

Utilisez l’URL HTTPS du serveur pour les clients distants, selon le [guide d’accès distant](remote.md). HTTP sur la boucle locale reste disponible. Les contrôles Host/Origin, l’admission des connexions et les limites de travail distant s’appliquent. Ce protocole fournit `apiKey` dans les paramètres d’URL ou de formulaire : évitez de conserver ces URL complètes et corps de formulaire dans les journaux ou captures partagées. Le serveur ne journalise pas ces requêtes ni secrets. Une clé par client facilite la révocation.

## Requêtes et réponses

GET et POST de formulaire sont disponibles ; POST exige `application/x-www-form-urlencoded`. HEAD est refusé avec HTTP 405, y compris pour les méthodes modifiant les données personnelles. Une requête authentifiée fournit `apiKey` (ou la clé via les champs anciens décrits ci-dessus), `v=1.16.1` et un nom de client `c` non vide. `f=json` choisit JSON ; XML UTF-8 avec l’espace de noms Subsonic est le défaut. Les réponses utilisent `subsonic-response`, la version de protocole `1.16.1`, le type serveur `Aede`, sa vraie `serverVersion` et `openSubsonic=true`. Les erreurs de protocole contiennent `status=failed` et `error.code/message`, généralement avec HTTP 200. Un média réussi utilise son type binaire et HTTP 200/206 ; une plage impossible renvoie HTTP 416. Une erreur de parsing avant sélection du format utilise XML. JSONP est refusé. Les réponses portent `no-store`.

UTF-8 et encodage pourcentage doivent être valides. Les options inconnues et champs simples dupliqués, même entre URL et corps, sont refusés. Seules les listes d’ID/dates précisées ci-dessous acceptent la répétition, dans l’ordre reçu. URL plus corps : 16 Kio ; valeur décodée : 2048 octets ; 32 noms de champs distincts et 256 occurrences au plus ; réception du corps : une seconde. Les grandes playlists demandent donc des ajouts par lots bornés. Le catalogue et les opérations personnelles utilisent les deux travailleurs partagés hors traitement réseau. La première vérification d’une clé utilise les deux travailleurs de mots de passe et les limites existantes de tentatives ; un cache borné à 128 empreintes évite de refaire Argon2 pour chaque requête valide. Chaque appel vérifie le store courant de comptes/clés ; les médias le revérifient pendant le transfert. Les écritures personnelles revérifient les droits sous le verrou partagé avant une sauvegarde atomique.

`getOpenSubsonicExtensions` est public, même si les comptes sont indisponibles, et annonce seulement `apiKeyAuthentication` et `formPost` version 1. Il accepte les paramètres communs sans exiger d’identifiants. `tokenInfo` renvoie le nom d’utilisateur associé à la clé.

La version de protocole du client doit avoir une version majeure égale à 1 et une version mineure inférieure ou égale à 16. Le troisième nombre doit être valide, mais ne participe pas au contrôle de compatibilité, conformément aux règles Subsonic.

## Méthodes disponibles

| Méthode | Paramètres supplémentaires et comportement |
| --- | --- |
| `ping`, `getLicense`, `tokenInfo` | Aucun. Licence valide, sans licence payante nécessaire. |
| `getUser` | `username` obligatoire, limité à soi-même. Utilisateurs/administrateurs diffusent, téléchargent, déclarent leurs écoutes et gèrent leurs playlists ; les auditeurs consultent seulement métadonnées/pochettes. Administration de l’installation indisponible. |
| `getMusicFolders` | Aucun. Une bibliothèque logique d’ID `1`, nommée Aède, sans exposer les racines du système de fichiers. |
| `getArtists` | `musicFolderId=1` facultatif ; index triés limités aux artistes associés à un album consultable. Submariner utilise la vue par artistes d’album décrite ci-dessous. |
| `getArtist`, `getAlbum`, `getSong` | `id` opaque obligatoire obtenu dans une réponse précédente. |
| `getAlbumList2` | `type` obligatoire parmi `alphabeticalByName`, `alphabeticalByArtist`, `byYear`, `byGenre`, `random` ; `size` (10 par défaut, maximum 500), `offset`, `musicFolderId=1` facultatifs. `byYear` exige `fromYear,toYear` ; des bornes inversées donnent des années décroissantes. `byGenre` exige `genre`. Ces filtres sont refusés avec les autres tris. Le paramètre Supersonic `limit` est un alias borné de `size` ; fournir les deux est une erreur. Chaque requête aléatoire mélange à nouveau ses résultats. Autres tris refusés. |
| `search3` | `query` obligatoire, éventuellement vide pour synchroniser. Options : `artistCount,artistOffset,albumCount,albumOffset,songCount,songOffset` et `musicFolderId=1`. Chaque comptage vaut 20 par défaut, maximum 1000 ; chaque décalage vaut 0 par défaut. Un comptage de zéro donne une catégorie vide. Recherche limitée à 1024 octets UTF-8. |
| `getGenres` | Aucun ; noms et nombres d’albums/pistes distincts, avec genre de l’album hérité par les pistes sans genre propre. |
| `getSongsByGenre` | `genre` obligatoire ; `count` (10 par défaut, maximum 500), `offset`, `musicFolderId=1` facultatifs. Pistes triées et paginées ; genre inconnu : liste vide. |
| `getRandomSongs` | `size` (10 par défaut, maximum 500), `genre`, `fromYear`, `toYear`, `musicFolderId=1` facultatifs. Pistes distinctes respectant tous les filtres fournis ; borne chronologique minimale inférieure ou égale à la maximale. |
| `getScanStatus` | Aucun. `scanning` suit les vrais scans synchrones, tâches HTTP et commandes CLI déléguées, jusqu’à publication du catalogue. `count` est le nombre de pistes du dernier catalogue publié, pas une estimation d’avancement. Aucun scan n’est démarré. |
| `getCoverArt` | `id` de pochette distinct reçu avec un album/une piste obligatoire ; `size=1..2048` facultatif. Pochettes externes JPEG/PNG cataloguées uniquement. Sans `size`, octets originaux vérifiés. Miniature éventuelle limitée à la réponse ; voir ci-dessous. |
| `stream`, `download` | `id` de piste obligatoire ; original binaire exact. Options acceptées : `format=raw`, `maxBitRate=0`, `timeOffset=0`, `converted=false`, `estimateContentLength` à `true` ou `false`. Autres conversions/options vidéo refusées. |
| `getStarred2` | `musicFolderId=1` facultatif ; uniquement vos artistes/albums/pistes favoris. |
| `star`, `unstar` | Au moins un `id`, `albumId` ou `artistId`, répétables avec les ID opaques correspondants ; mise à jour de vos annotations existantes. |
| `setRating` | `id,rating` obligatoires ; note artiste/album/piste de 0 à 5 ; 0 retire votre note. |
| `getPlaylists` | `username` facultatif, limité à soi-même ; playlists statiques privées uniquement. |
| `getPlaylist` | `id` de playlist obligatoire ; ordre et répétitions conservés. Une piste absente du catalogue échoue explicitement sans effacer sa référence enregistrée. |
| `createPlaylist` | `songId` répétable ; `name` obligatoire pour créer, ou `playlistId` pour remplacer les entrées d’une playlist privée existante. Liste vide acceptée. |
| `updatePlaylist` | `playlistId` obligatoire ; `name,comment,public=false` facultatifs, `songIdToAdd,songIndexToRemove` répétables. Les index de retrait désignent la liste initiale ; doublons/index hors limites refusés. |
| `deletePlaylist` | `id` de playlist obligatoire ; suppression limitée à votre playlist. |
| `scrobble` | `id` de piste répétable, dates Unix en millisecondes `time` facultatives de même nombre, et `submission=true` par défaut. Enregistre vos déclarations d’écoute et incrémente vos compteurs privés. `submission=false` accepte exactement une piste/date facultative pour un état de lecture temporaire. |
| `getNowPlaying` | Aucun ; déclarations courantes non expirées de vos clients uniquement, avec des clés toujours valides. |

Un album est une édition locale ; une chanson est une piste placée dans une édition, y compris les pistes sans album. Par défaut, les albums d’un artiste incluent les éditions qui le nomment comme artiste d’album ou artiste principal d’une de leurs pistes ou de leurs enregistrements. Un crédit principal de piste relie seulement sa propre édition ; les seuls crédits de compositeur, d’interprète ou d’invité ne créent pas de lien vers un album. `getArtists` et les résultats artistes de `search3` incluent uniquement les artistes possédant un tel lien vers un album. Cette vue reflète votre catalogue local, pas une discographie complète.

**Vue par artistes d’album pour Submariner.** Pour le nom de client exact `c=submariner` (sans distinction de casse ASCII), `getArtists` et les résultats artistes de `search3` listent uniquement les artistes désignés par l’`artistId` canonique d’au moins un album. `getArtist` renvoie seulement les albums canoniques de cet artiste, et `albumCount` suit la même règle. Les artistes explicitement marqués comme favoris restent visibles dans les favoris via `getStarred2`, y compris les artistes de pistes uniquement ; leur `albumCount` suit cette règle canonique et peut valoir zéro. Cela correspond au [parseur d’albums de Submariner 3.4](https://github.com/SubmarinerApp/Submariner/blob/v3.4/Submariner/SBSubsonicParsingOperation.swift#L282-L320), qui rattache un album à l’artiste annoncé. Il s’agit de distinguer crédits d’album et de piste, pas groupes et personnes. Une compilation reste sous son artiste d’album, souvent Various Artists ; les pistes de ses différents artistes restent accessibles par la recherche de pistes et la compilation. Les autres clients conservent la vue élargie décrite plus haut. Le graphe natif et l’artiste canonique de l’album restent inchangés.

Après mise à jour et redémarrage d’Aède, sélectionnez son serveur dans Submariner et utilisez **View → Reload Server** (⌘R), ou faites un clic droit sur le serveur puis **Reload Server**, pour demander des index actualisés. Submariner 3.4 peut aussi créer des artistes dans son propre cache à partir de recherches de pistes, de playlists, de navigation par dossiers ou d’informations de lecture en cours. Ces noms peuvent donc réapparaître sans album canonique, malgré l’index d’artistes filtré d’Aède. La consultation ordinaire d’un album ne crée pas ces entrées d’interprètes. Ce comportement appartient au cache du client ; Aède conserve les véritables références des artistes de pistes plutôt que de réattribuer les compilations.

Un artiste dont les seules pistes n’ont pas d’album reste accessible par la recherche de pistes et l’API native ; Aède ne fabrique pas d’album pour la navigation par artistes. Les ID sont des empreintes SHA-256 des références Aède : ils survivent à une renumérotation interne si les références restent les mêmes. Chemins de fichiers/pochettes, tags bruts et prose externe sont absents. Le champ obligatoire `created` d’un album est une approximation documentée : la date UTC du scan du catalogue, faute de date d’ajout indépendante ; elle ne sert pas à promettre un tri des derniers albums ajoutés. Les réponses de navigation incluent uniquement vos favoris/notes et compteurs de pistes, lorsqu’ils existent.

L’audio conserve exactement ses octets, sans DSP, normalisation, rééchantillonnage ni modification des tags. Au plus quatre transferts natifs/audio/pochettes fonctionnent ensemble, avec blocs/files bornés, arrêt à la révocation, à l’arrêt du serveur ou sur échec des contrôles de source. Une source audio doit rester un fichier ordinaire du catalogue, avec taille/date précise d’un scan courant ; une source modifiée ou d’identité ancienne imprécise demande un nouveau scan. Une seule plage `Range: bytes=…` sélectionne des octets audio originaux. Sans validateur fort reconnu, `If-Range` déclenche une réponse complète pour éviter une reprise mélangeant des fichiers différents. Les auditeurs lisent métadonnées/pochettes, mais ne diffusent/téléchargent pas l’audio.

Ce transport ne décode pas le FLAC et ne vérifie donc pas son MD5 audio décodé. Ce contrôle appartient au décodeur du client choisi, s’il le prend en charge. Un transfert d’octets réussi ou un scrobble client ne prouve pas une vérification du contenu décodé. La [lecture PCM native](playback.md) d’Aède vérifie séparément l’empreinte FLAC stockée lorsque son décodage complet atteint la fin.

La pochette doit être un fichier JPEG/PNG ordinaire dans le dossier natif de l’édition ; liens de source, chemins sortant du dossier, fichiers corrompus et dépassements des limites d’image existantes sont refusés. La validation lit tous les pixels, dans les limites de 32 Mio d’entrée, 8192 pixels par axe, 16 millions de pixels et de mémoire décodée existantes. `size` borne le plus grand axe sans changer les proportions ni agrandir l’image. Si une réduction est nécessaire, la réponse contient une miniature PNG calculée en mémoire ; sinon les octets originaux sont renvoyés. Les pochettes sources, images 4K téléchargées et fichiers audio ne sont jamais réécrits. Cet adaptateur n’annonce pas encore les images intégrées aux tags ni les autres familles d’illustrations.

## Données personnelles et déclarations d’écoute

L’adaptateur traduit directement `setRating` vers l’annotation existante du compte authentifié : 1 à 5 enregistre cette note et 0 la retire. `star`/`unstar` active ou retire séparément le favori. Ces requêtes mettent à jour le modèle d’annotations existant d’Aède sans lancer de commande CLI. Subsonic et l’API native lisent les données personnelles du compte authentifié ; les commandes CLI ordinaires consultent le propriétaire local, conservé par le premier administrateur. Le stockage est commun, tandis que les annotations des autres comptes restent isolées.

Favoris et notes utilisent les annotations existantes. La date `starred` est leur dernière modification, une approximation explicite faute de date de favori indépendante ; changer une note peut changer cette date affichée. Les playlists statiques sont distinctes des collections de requêtes. Elles utilisent des références stables dans `user.json`, conservent les répétitions et gardent les références manquantes jusqu’à une modification explicite. Nom non blanc de 256 octets UTF-8 au plus ; commentaire de 2048 octets au plus. Le store accepte 128 playlists par propriétaire, 512 au total et 2000 entrées ordonnées par playlist. Partage et sélection d’un autre propriétaire sont refusés, même pour les administrateurs. Arguments invalides ou stores indisponibles ne provoquent pas de sauvegarde partielle.

Diffuser seul un fichier ne crée aucune écoute. Un scrobble soumis conserve une déclaration client et sa date ; aucune durée lue, fin, décodage audio ou confirmation de consommation n’est inventée. Le journal récent conserve les 500 dernières déclarations, répétitions incluses ; les compteurs durables incluent aussi les écoutes natives confirmées et survivent à la rotation du journal. La date vaut celle de réception par défaut et ne peut pas dépasser un jour dans le futur. Une soumission n’est pas idempotente : répéter une requête réussie peut incrémenter à nouveau le compteur. Un état de lecture ne crée ni historique ni incrément. Le serveur garde un état par clé en mémoire, 128 au total, avec expiration selon la durée connue (de 30 secondes à 24 heures ; cinq minutes si inconnue). Révocation/changement de compte masquent immédiatement cet état ; un redémarrage les efface tous.

## Premier essai macOS avec Supersonic

Installez une version macOS depuis les [versions officielles de Supersonic](https://github.com/supersonic-app/supersonic/releases), en suivant ses [consignes d’installation macOS](https://github.com/supersonic-app/supersonic#mac-os) ; le projet décrit une étape de lancement supplémentaire pour son application non notariée. Analysez votre musique, initialisez les comptes si nécessaire, créez une clé client distincte et redémarrez le serveur recompilé avec le même dossier de données :

```sh
aede accounts keys alice create "Supersonic Mac"
aede serve --port 8787
```

Remplacez `alice` par votre compte existant. Depuis ce dépôt, la compilation de vérification produit `./target/release/aede` : utilisez ce chemin à la place de `aede` pour tester l’exécutable actuel. Si votre catalogue utilise un dossier personnalisé, ajoutez le même `--data "/chemin/vers/donnees"` aux deux commandes. Aède fournit lui-même le serveur compatible ; aucune installation d’un serveur Subsonic séparé n’est nécessaire.

Dans le dialogue serveur de Supersonic, choisissez **Subsonic**, l’adresse `http://127.0.0.1:8787` sur ce Mac (sans `/rest`), votre compte Aède dans **Username**, et la clé entière `id.secret` dans **Password**. Cochez **Use legacy authentication**. Ce [réglage exact du client](https://github.com/supersonic-app/supersonic/blob/0eb34945c59e1d010ef618a0dedd3bc1edd37908/ui/dialogs/addeditserverdialog.go#L54) envoie la clé dans `p`, au lieu de fabriquer un jeton MD5 non pris en charge. Gardez le transcodage désactivé pour demander l’audio original.

Dans la page **Albums**, sélectionnez **Title (A-Z)** dans le menu de tri. Le défaut Recently Added de Supersonic appelle `newest`, refusé explicitement tant qu’Aède n’a pas de vraie date d’ajout ; le [tri Title (A-Z)](https://github.com/supersonic-app/supersonic/blob/0eb34945c59e1d010ef618a0dedd3bc1edd37908/backend/mediaprovider/subsonic/albumiterator.go#L93) utilise `alphabeticalByName`. Années, genres et lecture aléatoire sont aussi pris en charge. Commencez par un album au format que le client sait décoder, puis vérifiez pochettes, déplacement dans la lecture, favoris, playlists et compteurs déclarés. Sur un téléphone/autre ordinateur, utilisez l’adresse HTTPS configurée, pas la boucle locale de ce Mac.

## Configuration locale de Submariner sur macOS

Pour [Submariner 3.4](https://github.com/SubmarinerApp/Submariner/tree/v3.4), utilisez le même transport de clé client. Son [authentification](https://github.com/SubmarinerApp/Submariner/blob/v3.4/Submariner/SBServer.swift) envoie `u+t+s` quand **Use Token-Based Authentication** est coché, et `u+p=enc:<hex>` quand il est décoché. Seul ce second mode peut transporter une clé d’application Aède.

**La commande de création de clé est obligatoire avant de remplir les réglages du client.** Suivez ces étapes dans l’ordre depuis le dossier contenant l’exécutable `aede`. Depuis la racine du dépôt, remplacez `./aede` par `./target/release/aede` ; si Aède est dans votre `PATH`, utilisez simplement `aede`.

1. Vérifiez que les comptes sont initialisés et choisissez un compte administrateur ou utilisateur actif. Si nécessaire, suivez d’abord la [configuration des comptes](../cli/accounts.md).

   ```sh
   ./aede accounts list
   ```

2. **Créez la clé pour Submariner**, en remplaçant le nom d’exemple `alice` par le compte choisi :

   ```sh
   ./aede accounts keys alice create "Submariner Mac"
   ```

3. Copiez la valeur complète `id.secret` de 129 caractères après `API key (shown once):`. Gardez le point et les deux parties ; ne copiez ni l’ID seul du tableau ni le libellé `API key (shown once):`. Elle n’est affichée qu’une fois. N’utilisez pas le mot de passe du compte.

4. Avant la première lecture depuis le client, actualisez le catalogue existant par un scan normal :

   ```sh
   ./aede scan
   ```

   Cette commande reprend les dossiers musicaux suivis et actualise les anciennes identités de fichiers sans date de modification précise. Ni `--full` ni `--replace` ne sont nécessaires. Fichiers musicaux et tags restent intacts ; les dossiers suivis temporairement hors ligne conservent leurs entrées dans le catalogue. Leur audio redevient disponible seulement lorsqu’ils sont de nouveau accessibles. Voir [scan](../cli/scan.md).

5. Démarrez le serveur sur ce Mac s’il ne fonctionne pas déjà :

   ```sh
   ./aede serve --port 8787
   ```

   Utilisez le même dossier `--data` pour les quatre commandes s’il est personnalisé. Un serveur déjà démarré prend en compte la nouvelle clé et le scan publié sans redémarrage. S’il utilise un autre port, reprenez l’adresse qu’il affiche au lieu du port d’exemple `8787`.

6. Ajoutez le serveur dans Submariner avec ces réglages :

   | Champ | Valeur |
   | --- | --- |
   | Server Name | `Aède`, ou le nom de votre choix |
   | URL | `http://127.0.0.1:8787` |
   | Username | Le compte utilisé à l’étape 2, par exemple `alice` |
   | Password | La clé `id.secret` complète de 129 caractères de l’étape 3 |
   | Use Token-Based Authentication | Décoché |

L’adresse ne comporte pas de suffixe `/api/v1/status` ou `/rest`. Laissez **Use Token-Based Authentication décoché** : Submariner encode lui-même la clé ; n’ajoutez pas `enc:` et ne l’encodez pas manuellement en hexadécimal. Aède fournit déjà le serveur : aucune installation Subsonic séparée n’est nécessaire.

Si Aède renvoie **`enc: must encode a complete API key`**, le champ mot de passe ne contenait pas une clé complète : un mot de passe de compte, l’ID seul ou un collage tronqué ne fonctionnent pas. Créez une clé avec l’étape 2 si ce n’est pas encore fait, puis remplacez le mot de passe du client par la valeur entière de l’étape 3 et vérifiez que l’authentification par jeton est décochée. Un secret perdu doit être remplacé ; `accounts keys alice` liste des ID, pas des clés complètes utilisables.

Si **Play reste à 00:00** et qu’Aède signale **`Media source is unavailable or changed; refresh the catalog`**, vérifiez que le dossier musical est accessible, puis répétez le scan normal `./aede scan` de l’étape 4 avec le dossier de données du serveur. Un ancien catalogue sans horodatages précis doit être actualisé avant la diffusion ; une source modifiée ou indisponible reste refusée tant que son identité courante ne peut pas être vérifiée. Ce contrôle des sources reste actif et ne réécrit pas l’audio.

L’authentification locale, la navigation par artistes/albums, la lecture du FLAC original, les notes et les favoris artistes/albums avec Submariner 3.4 sur macOS ont été confirmées par un essai réel du client. Les autres formats audio, les panneaux facultatifs, l’accès distant et la capacité sur NAS restent à valider ; la navigation par dossiers et certains tris peuvent appeler des méthodes indisponibles. Cet essai ne valide pas toute la compatibilité de Submariner.

## Compatibilité restant à développer

Restent les tris d’albums `newest/highest/frequent/recent/starred`, l’ancienne navigation par dossiers et `getStarred`, la sauvegarde de file d’écoute, les informations détaillées artiste/album, les rapports OpenSubsonic de lecture, les pochettes intégrées, le transcodage, les podcasts, radios, vidéo et administration distante. Appels/options indisponibles échouent explicitement. Les chemins initiaux de navigation/lecture de Supersonic ont été étudiés dans ses sources officielles ; ses panneaux facultatifs qui demandent ces routes absentes peuvent encore échouer. Navigation/lecture réelles complètes, capacité NAS et compatibilité par application restent à tester. Le client choisi doit lire le format original. Symfonium sur Android prend en charge les clés API selon sa [note de version officielle](https://support.symfonium.app/t/version-11-6-0-beta-1/6358) ; son [guide fournisseur](https://docs.symfonium.app/wiki/providers/subsonic-opensubsonic-media-provider-configuration/) décrit la synchronisation rapide et les fichiers originaux, mais il n’a pas encore été testé avec Aède.
