# Démarrer le serveur local

Le serveur d’Aède permet à d’autres programmes de consulter votre **catalogue** musical : albums, artistes, pistes, relations et diagnostics. Il répond en JSON et annonce les changements par WebSocket. Les [comptes et sessions](accounts.md) facultatifs protègent l’accès local et isolent les données personnelles. Un [contrat audio authentifié](playback.md) transmet le PCM traité ; une [configuration HTTPS explicite](remote.md) active l’accès distant avec comptes obligatoires.

Pour connecter Submariner ou un autre client Subsonic/OpenSubsonic, créez d’abord une clé d’application selon le [guide de configuration client](subsonic.md). Le mot de passe du compte Aède ne permet pas cette connexion.

<div id="a-dedicated-home-for-the-library" data-legacy-anchor></div>

## Un hôte dédié à la bibliothèque

Pour une collection durable ou partagée, Aède est conçu autour d’un serveur dédié, d’un NAS ou d’un petit ordinateur toujours allumé. Musique, données du catalogue et tâches de fond restent sur cet hôte ; les lecteurs s’y connectent depuis vos appareils du quotidien. La bibliothèque dispose ainsi de chemins stables et reste disponible indépendamment de la veille ou du remplacement du portable, avec des sauvegardes gérées sur son hôte.

Le moteur fonctionne sans bureau graphique, par sa CLI et son API serveur. Les lecteurs graphiques, dont la future interface Phémios, sont des clients séparés ; le moteur évolue comme un service plutôt que comme une application de bureau tout-en-un. La CLI fonctionne aussi sur un ordinateur du quotidien pour un usage local, une évaluation ou l’administration. Consultez l’[état des déploiements](#nas-raspberry-pi-et-conteneurs) avant de choisir un hôte : paquets NAS et matériel cible restent à valider.

## Avant de démarrer

Installez Aède et analysez au moins un dossier musical avec `scan`. La CLI et le serveur doivent partager le même dossier de données persistant. Aède lit la musique et ses tags sans réécrire les originaux.

```sh
aede scan /chemin/vers/musique
aede serve
```

Remplacez `/chemin/vers/musique` par votre dossier. Gardez le second terminal ouvert : `serve` fonctionne jusqu’à son arrêt. Sans catalogue enregistré, le démarrage échoue ; commencez par le scan. Le serveur affiche son adresse lorsqu’il est prêt.

L’adresse par défaut est `http://127.0.0.1:8787`. `127.0.0.1`, appelée adresse de boucle locale, désigne **cet ordinateur**. Sur un téléphone, elle désigne le téléphone, pas votre ordinateur musical. HTTP sans chiffrement est limité à la boucle locale. Le [guide HTTPS](remote.md) explique comment configurer une écoute authentifiée sur une autre adresse.

```sh
curl http://127.0.0.1:8787/api/v1/status
```

`curl` est un client HTTP en ligne de commande. Cette requête doit renvoyer un objet JSON avec `status` à `ok` et `catalog_loaded` à `true`. Si votre terminal ne trouve pas `curl`, installez le client HTTP de votre système ou utilisez un client d’API local. Un navigateur peut afficher cette adresse JSON publique sur le même ordinateur ; l’interface d’administration impose d’autres restrictions.

## Conserver un emplacement de données unique

Avec un dossier personnalisé, répétez l’option générale `--data` pour les deux commandes :

```sh
aede --data /chemin/vers/donnees-aede scan /chemin/vers/musique
aede --data /chemin/vers/donnees-aede serve --port 3412
```

Vous pouvez aussi définir `AEDE_HOME` dans l’environnement des deux processus. Sans choix explicite, Aède utilise `$XDG_DATA_HOME/aede`, puis `~/.local/share/aede` ; sans `HOME`, il utilise `.aede` dans le dossier courant. Les chemins musicaux sont ceux de l’ordinateur qui exécute le serveur.

`--port` accepte 0 à 65535. En HTTP, la valeur 0 demande un port disponible au système : lisez l’adresse affichée au lieu de supposer qu’il s’agit de 8787. TLS exige un port non nul et une adresse publique explicite. Après une mise à jour de l’exécutable, redémarrez le serveur pour utiliser la nouvelle version.

## Arrêter le serveur et utiliser la CLI

Ctrl-C dans le terminal du serveur demande un arrêt propre. Un gestionnaire de services peut envoyer SIGTERM. Les tâches acceptées peuvent retarder cet arrêt ; annulez explicitement un long scan ou fetch si vous ne souhaitez pas le laisser finir. Fermer un client HTTP n’annule pas une tâche acceptée.

Sur Unix, les commandes CLI qui modifient les données avec le même compte et le même dossier sont automatiquement déléguées au serveur par un canal local privé. Elles conservent leur affichage habituel. Les scans/fetch délégués affichent un identifiant ; `aede cancel <task-id>` demande leur annulation. Les tâches HTTP se suivent et s’annulent par HTTP : les deux interfaces ne sont pas interchangeables. La délégation par socket local est indisponible sous Windows.

Ne supprimez pas les fichiers de verrouillage et ne modifiez pas les JSON pendant leur utilisation. La CLI et le serveur coordonnent leurs écritures avec le même verrou. Voir les [tâches HTTP](jobs.md) et le [guide d’exploitation](../../operating.md) pour leur durée de vie et les sauvegardes.

## NAS, Raspberry Pi et conteneurs

La séparation entre musique, catalogue persistant et clients rend pertinent un petit hôte toujours allumé. Les contrats de catalogue, HTTPS et audio natif implémentés ne constituent pas un appareil NAS validé. Aucune image de conteneur Aède n’est construite ou publiée ; les paquets NAS, le déploiement Raspberry Pi, le redémarrage de l’hôte et les performances sur ces appareils restent à valider. Les versions Linux publiées ciblent actuellement x86_64, pas Linux aarch64.

Le [guide d’exploitation](../../operating.md#docker-image-on-a-linux-host-procedure-only) décrit une procédure pour un futur conteneur Linux, avec chemins stables, permissions et sauvegardes. Son nom d’image fictif ne désigne pas une image disponible au téléchargement. L’accès distant exige l’[écoute HTTPS](remote.md) explicite avec comptes ; l’écoute HTTP par défaut reste locale.

Suite : [Comprendre HTTP et JSON](http.md), puis [Parcourir les albums et pistes](catalog.md).
