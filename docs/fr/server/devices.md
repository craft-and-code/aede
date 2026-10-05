# Appareils audio réseau

M4 commence par une lecture explicite et finie depuis le Terminal : SlimProto d’abord, puis UPnP AVTransport et OpenHome Playlist. Ce sont de premiers profils de contrôle, pas un serveur Lyrion complet, une certification DLNA ni une garantie pour tous les appareils. La validation sur du matériel réel reste à faire.

## Où se fait la lecture

`aede play` décode localement, applique le DSP partagé et utilise normalement CPAL pour la sortie audio du système. `aede cast` transmet le **fichier original inchangé** par HTTP à un seul lecteur du réseau local. Ce lecteur le décode et gère sa sortie et son horloge. Aucune normalisation, égalisation, conversion de fréquence, dither ni compression avec pertes n’est appliqué côté serveur à ces originaux. Un format non annoncé comme compatible est refusé plutôt que converti silencieusement. Le lecteur peut avoir son propre traitement.

La vérification MD5 du FLAC décodé, effectuée pendant la lecture locale ou PCM native, ne s’exécute pas lors de ce transfert du fichier encodé : Aède ne décode pas la source ici. Les contrôles du décodeur dépendent du lecteur destinataire ; une vérification explicite de l’intégrité de la bibliothèque reste une opération distincte.

La sélection reprend celle de la lecture locale : fichier, dossier, M3U/M3U8, collection ou artiste/album/piste catalogué. Ce premier profil accepte 1 à 64 occurrences, dans l’ordre, en conservant les doublons volontaires. Il n’ajoute ni répétition, aléatoire, déplacement dans le morceau ni édition directe de file au Terminal. Garder la commande ouverte pendant la lecture. Ctrl-C demande Stop au lecteur et ferme la distribution HTTP. Un appareil déconnecté ne peut pas confirmer Stop ; un téléchargement ou état déclaré par l’appareil ne crée pas d’écoute dans l’historique.

Ce contrôleur temporaire est distinct de `aede serve` : il ne demande pas d’API déjà lancée et n’expose ni catalogue, comptes ni administration. Le registre permanent d’appareils et leur contrôle authentifié par une interface graphique viendront dans une étape suivante.

Le crate autonome [`aede-devices`](../../../crates/aede-devices/README.md) implémente ces protocoles et leur distribution HTTP des originaux sélectionnés. Le Terminal l’appelle directement ; Subsonic/OpenSubsonic et les autorisations de comptes restent dans `aede-server`. Les deux transports partagent les mêmes règles de types MIME et de plages d’octets. Le chemin Rust existant `aede_server::devices` réexporte le nouveau crate, sans seconde implémentation.

## SlimProto

Sur le serveur, choisir son interface et l’adresse du lecteur attendu :

```sh
aede cast /chemin/vers/album --protocol slimproto --bind 192.168.1.10 --device 192.168.1.20
```

Sur la machine équipée de Squeezelite, le connecter à ce serveur :

```sh
squeezelite -s 192.168.1.10:3483
```

Pour un essai logiciel local, les deux adresses peuvent être `127.0.0.1`. Démarrer Aède d’abord : il attend le lecteur sélectionné pendant une minute au maximum. `--port N` change le port TCP SlimProto. Autoriser ce port et le port média temporaire affiché au lancement dans le pare-feu local. La découverte SlimProto UDP, l’API de contrôle HTTP/JSON de Lyrion et l’émulation des écrans ne sont pas fournies.

La connexion vérifie les codecs annoncés. Ce premier profil accepte FLAC, MP3 et WAV PCM entier, mono/stéréo, avec des paramètres pris en charge explicitement. Le WAV flottant et les formats inconnus/non pris en charge sont refusés. `MaxSampleRate` limite la sélection ; en son absence, les fréquences supérieures à 48 kHz sont refusées par prudence. La résolution FLAC doit être connue et limitée à 24 bits dans ce premier profil. Le WAV haute fréquence doit aussi avoir un code de fréquence SlimProto représentable. Toute la sélection est vérifiée avant le premier démarrage.

La fin du téléchargement ou du décodage ne signifie pas que la sortie a fini. Le passage au suivant attend les confirmations du démarrage, du décodage terminé et de la sortie vidée. Le gapless SlimProto n’est donc pas promis. Des interrogations régulières et des délais de progression bornés détectent une perte de connexion. Pause/boutons du lecteur et multiroom synchronisé restent hors de ce premier contrôleur.

## UPnP AV et OpenHome

Découvrir les lecteurs annoncés sur une interface :

```sh
aede devices --bind 192.168.1.10
```

Reprendre la **Description URL** affichée, sans inventer une adresse de contrôle :

```sh
aede cast /chemin/vers/album --protocol upnp --bind 192.168.1.10 --device http://192.168.1.20:49152/device.xml
aede cast /chemin/vers/album --protocol openhome --bind 192.168.1.10 --device http://192.168.1.20:49152/device.xml
```

UPnP utilise ConnectionManager/GetProtocolInfo et AVTransport/SetAVTransportURI/Play, avec des métadonnées DIDL-Lite. Un lecteur actif doit être arrêté d’abord. La file passe au suivant après confirmation de lecture puis d’arrêt. `SetNextAVTransportURI` n’est pas utilisé : le gapless n’est pas garanti. Les erreurs sont signalées.

ConnectionManager doit appartenir au même lecteur qu’AVTransport, y compris dans une description avec appareils intégrés. Aède vérifie GetPositionInfo/TrackURI avant de passer au suivant : une URI étrangère, ou effacée après confirmation de lecture, termine le contrôleur sans envoyer Stop à un son qu’il ne contrôle plus. Un appareil qui efface son URI en fin de piste nécessitera donc un profil de compatibilité validé séparément pour permettre l’enchaînement ; ce refus prudent évite de reprendre la lecture d’un autre contrôleur.

OpenHome vérifie Playlist/ProtocolInfo, TracksMax, répétition/aléatoire et file existante avant d’insérer la sélection ordonnée. Une file déjà remplie est refusée, sauf avec **`--replace`**, qui autorise explicitement son remplacement. Répétition ou aléatoire déjà actif sur l’appareil est refusé plutôt que changé silencieusement. L’appareil conserve la file ; Aède reste actif pour servir les originaux et détecter les modifications. Une modification extérieure termine le contrôleur. Si l’insertion échoue après certaines entrées, elles peuvent rester sur l’appareil : Stop est demandé et l’erreur signalée, sans restaurer une ancienne file. UPnP/OpenHome ne modifient pas le volume. SlimProto conserve également le gain par défaut ; `--device-volume 0..100` demande explicitement un pourcentage de gain numérique linéaire (0 coupe le son, 100 est le gain unité). Un nouveau processus Squeezelite peut démarrer muet : ajouter, par exemple, `--device-volume 20` à la commande Aède, puis ajuster volontairement. Ce pourcentage ne garantit pas un volume acoustique ni une échelle logarithmique.

Un type MIME compatible `http-get` doit être annoncé. Cette déclaration ne garantit pas les limites de canaux, résolution, fréquence ou firmware : les vérifier sur le lecteur réel. Les simulations logicielles vérifient les messages et les limites. La découverte SSDP M-SEARCH reste explicite, bornée et absente des commandes normales du catalogue.

Les entrées OpenHome terminées restent sur l’appareil, mais leurs URL temporaires expirent quand Aède se ferme. Les rejouer demande une nouvelle session : cette file ne constitue pas une bibliothèque persistante accessible depuis l’appareil.

## Limites réseau et fichiers

`--bind` et le lecteur utilisent des adresses IPv4 précises, privées, de lien local ou de boucle locale. Écoute sur toutes les interfaces, adresses publiques, DNS et IPv6 ne font pas partie de ce premier profil. Les descriptions/contrôles utilisent HTTP, sans identifiants ni fragments. Une description découverte doit correspondre à l’émetteur SSDP ; URLBase et contrôles gardent son hôte et son port. Redirections, traversées de chemin, DTD/entités XML personnalisées et messages surdimensionnés sont refusés.

Certains lecteurs anciens ne peuvent pas utiliser l’API HTTPS authentifiée d’Aède. Leur serveur média HTTP temporaire utilise donc un secret aléatoire de session, l’IP exacte du lecteur et des routes limitées à la sélection, sans accès général aux fichiers ni à l’API. Garder les URL secrètes. HTTP et ces contrôles sont réservés au réseau local de confiance, sans publication Internet. L’IP limite l’admission mais ne constitue pas une identité cryptographique.

Les originaux sont ouverts en lecture seule. Taille, date précise, statut de fichier ordinaire et identité Unix sont vérifiés à la préparation, à l’ouverture et pendant le transfert. Un fichier changé ou remplacé par un lien arrête le transfert. Quatre requêtes au maximum détiennent les places de transfert ; en-têtes, délais et files de deux blocs de 32 Kio bornent le travail et la mémoire. Byte ranges et HEAD reprennent la politique existante de distribution des originaux. Une opération du système de fichiers ou du pilote peut encore se bloquer ; l’arrêt du contrôleur borne l’attente du runtime sans promettre d’interrompre le système d’exploitation. Fichiers et tags ne sont jamais réécrits.

## Validation et suite de M4

Tester de vrais Squeezelite/Squeezebox et lecteurs UPnP/OpenHome : FLAC/MP3/WAV, déplacement effectué par le lecteur, pause/reprise et déconnexion/reconnexion. Capturer la sortie sous charge avant toute garantie gapless ou multiroom. Ensuite : enchaînements continus, contrôles/métadonnées plus riches, registre d’appareils, API authentifiée et politique explicite de preuve d’écoute. Parcourir Aède depuis un autre contrôleur DLNA demande aussi MediaServer/ContentDirectory ; contrôler un lecteur ne fournit pas ce rôle serveur.

La [roadmap](../../design/roadmap.md) distingue ces premiers profils des protocoles optionnels. Références primaires : [Squeezelite](https://github.com/ralph-irving/squeezelite), [spécifications UPnP AV](https://openconnectivity.org/developer/specifications/upnp-resources/upnp/mediaserver4-and-mediarenderer3/) et [service OpenHome Playlist](https://github.com/openhome/ohNet/blob/master/OpenHome/Net/Service/Upnp/OpenHome/Playlist1.xml).

## Essais par la communauté

Ces premiers adaptateurs restent expérimentaux jusqu’à validation par des retours matériels. Squeezelite permet un premier essai logiciel sans acheter de streamer ; choisir volontairement `--device-volume` s’il démarre avec un gain muet. Cela ne remplace pas une mesure sur une sortie physique.

Pour un retour utile : version/commit Aède, système, modèle/firmware du lecteur ou version de Squeezelite, protocole choisi, codec/conteneur, fréquence, résolution et nombre de canaux. Décrire connexion, premier son, passage de piste, pause/déplacement côté lecteur, fin de file et Ctrl-C. Joindre l’erreur exacte et une commande anonymisée. Retirer URL secrètes de session, noms privés et chemins de bibliothèque des captures/journaux.

Commencer avec un FLAC stéréo ordinaire, puis un album, un M3U avec entrée répétée, MP3 et WAV entier pris en charge. Essayer une déconnexion pendant le transfert et une file modifiée sur l’appareil. Décrire le passage entre pistes sans parler de gapless mesuré en l’absence d’une capture appropriée. Utiliser un fichier synthétique ou non privé pour une reproduction. Conserver le premier problème observé plutôt que changer plusieurs réglages à la fois.
