<div id="operating-the-local-server" data-legacy-anchor></div>

# Exploiter le serveur local

Le serveur M2 d’Aède est une **API de catalogue locale**, pas encore un serveur musical distant. Il sert du JSON et des notifications WebSocket, sans diffuser l’audio ni authentifier les auditeurs. La procédure de publication construit des archives macOS Apple Silicon, Linux x86_64 et Windows x64. La CI Windows native valide scan, fichiers annexes et copies ; la délégation locale par socket Unix reste indisponible sous Windows. Ces builds ne prouvent pas un déploiement validé en service ou sur NAS pour chaque plateforme. Voir les [chemins](../design/paths.md).

<div id="start-and-stop" data-legacy-anchor></div>

## Démarrer et arrêter

Choisissez un dossier de données persistant unique pour la CLI et le serveur. Définissez `AEDE_HOME` dans l’environnement du service ou fournissez toujours `--data <directory>`. Sans ces choix, Aède utilise `$XDG_DATA_HOME/aede`, puis `~/.local/share/aede` ; sans `HOME`, `.aede` dans le dossier courant. Le compte du service doit lire les dossiers musicaux et lire/écrire ses données ; audio et tags ne sont jamais réécrits. Les commandes déléguées créant des fichiers près de la musique, telles que `fetch --lyrics`, téléchargement d’images ou `analyze --json`, nécessitent aussi l’écriture dans leur destination. Ces écritures utilisent les permissions du compte serveur.

```sh
aede scan /path/to/music
aede serve
```

Le premier scan crée `catalog.json`. Serve refuse de démarrer sans lui. Le serveur affiche son adresse réelle lorsqu’il est prêt. Il écoute par défaut sur `127.0.0.1:8787` ; `--port` accepte 0–65535 et `aede serve --port 0` choisit/affiche un port local disponible. Vérification rapide : `curl http://127.0.0.1:8787/api/v1/status`. Arrêtez un serveur au premier plan par Ctrl-C ; un gestionnaire de services peut envoyer SIGTERM. L’arrêt ferme nouvelles connexions et requêtes de commandes incomplètes, puis attend les opérations acceptées. Un long fetch/check délégué peut retarder la sortie ; annulez un scan/fetch délégué s’il ne doit pas finir. Prévoyez assez de temps dans le gestionnaire et arrêtez proprement avant le NAS. Le [contrat d’API](api.md) liste routes, événements et erreurs ; `aede help serve` donne un guide hors ligne.

L’API HTTP de catalogue est en lecture seule. Le [README du serveur](../../crates/aede-server/README.md) liste chaque route et ses paramètres, notamment albums, artistes, origines, pistes, diagnostics et recherche. Redémarrez le serveur après mise à jour de l’exécutable pour activer les nouvelles routes.

Pour autoriser les tâches scan/fetch administratives et les écritures personnelles, fournissez dans l’environnement serveur un `AEDE_ADMIN_TOKEN` ASCII privé d’au moins 32 caractères **avant** le démarrage. Conservez-le dans les secrets du gestionnaire ou une configuration locale protégée, pas en URL, page de navigateur ou exemple publié. `POST /api/admin/v1/scan` ou `/api/admin/v1/fetch` avec un objet JSON renvoie un ID ; suivez `GET /api/admin/v1/tasks/{id}` et annulez par un `POST /api/admin/v1/tasks/{id}/cancel` vide. Le même secret protège annotations, historique et collections, associés au propriétaire unique `local` jusqu’aux futurs comptes. Toutes exigent `Authorization: Bearer <token>` et refusent les origines de navigateur ; sans jeton, elles sont absentes. Le scan sans corps reste synchrone par compatibilité. Un scan CLI ordinaire du même compte ne demande aucun jeton administratif.

Les tâches HTTP survivent à la déconnexion ; l’arrêt propre les attend. ID et stdout/stderr bornés vivent seulement en mémoire (quatre tâches actives, 64 dossiers maximum), pas au-delà d’un redémarrage. L’annulation conserve le travail sauvegardé. Fetch utilise les clés du serveur sans pouvoir demander de saisie ; fournissez explicitement `yes` pour accepter un grand traitement. Dossiers de scan et cibles de dossiers fetch sont des chemins du serveur. Suivi/annulation HTTP concernent seulement les tâches soumises en JSON ; utilisez `aede cancel` pour celles déléguées par CLI. Ne répétez pas aveuglément une soumission dont la réponse est perdue : la tâche peut être active.

Les nouveaux fichiers annexes d’images/paroles sont publiés atomiquement, sans remplacer un existant, même créé simultanément par un autre logiciel. Leur système de fichiers doit accepter les liens physiques ; FAT/exFAT ne les accepte pas, par exemple. Sinon le téléchargement échoue plutôt qu’utiliser un écrasement risqué. Un échec/interruption ne publie jamais de fichier final partiel, mais peut laisser un temporaire caché.

<div id="cli-alongside-the-server" data-legacy-anchor></div>

## Utiliser la CLI avec le serveur

Sur Unix, les commandes pouvant écrire pour le même compte/dossier s’exécutent automatiquement sous le serveur actif par socket privé. Elles gardent leur sortie habituelle ; un long fetch continue si sa CLI se déconnecte. Scan/fetch délégués affichent un ID. `aede cancel <task-id>` avec les mêmes data/AEDE_HOME demande explicitement l’arrêt ; fermer le terminal ou Ctrl-C sur la CLI **n’annule pas** la tâche serveur. La demande répond immédiatement et la CLI originale sort en code 130 une fois arrêtée. Les réponses déjà sauvegardées par fetch restent. Les ID valent jusqu’au redémarrage ; tâches terminées, scans administratifs HTTP et autres commandes ne s’annulent pas ainsi. Sans serveur, les commandes sont locales, Ctrl-C arrête ce processus et cancel indique aucun serveur disponible.

Le dossier de données ne doit être accessible en écriture ni aux autres utilisateurs ni aux groupes. Le socket privé utilise un dossier protégé sous `/tmp` ; gardez `.aede-server.lock` et le verrou des données. Le serveur limite les connexions de commandes locales et refuse à saturation. Une requête incomplète expire ; une CLI ne lisant plus sa sortie se déconnecte après délai, mais la commande acceptée continue. Réessayez une requête refusée après les autres tâches ; une commande acceptée puis déconnectée ne doit pas être soumise à nouveau aveuglément.

Tous les rédacteurs Aède actuels partagent le verrou des données, y compris la CLI sans serveur. Ne supprimez pas `.aede.lock`, n’éditez pas le JSON pendant Aède et ne mélangez pas anciens exécutables et serveur actuel. Un verrou concurrent fait répondre `409 store_busy` au scan HTTP synchrone ou doctor ; les tâches HTTP asynchrones l’attendent. Le serveur détecte un `catalog.json` remplacé en environ une seconde. Un remplacement mal formé laisse le dernier instantané valide et journalise une erreur ; retirer le catalogue rend les routes indisponibles jusqu’à sa restauration.

<div id="backups-and-recovery" data-legacy-anchor></div>

## Sauvegarde et récupération

Sauvegardez le dossier de données sur un stockage persistant. Il contient `catalog.json` (graphe reconstructible), `conclusions.json` (intégrité et analyses importées), `user.json` (annotations personnelles), `sources.json` (informations externes attribuées) et les ressources dérivées. `aede backup <file>` produit une sauvegarde versionnée des stores JSON ; gardez des copies hors NAS. Ce fichier **ne sauvegarde pas** la musique originale ni les images/paroles dérivées. Dossier et sauvegarde sont privés : historique, chemins et informations téléchargées peuvent y figurer.

Pour récupérer, arrêtez le serveur, préservez les données endommagées et lancez `aede restore <file>` avec les mêmes AEDE_HOME/data. Lisez la confirmation avant d’accepter. Restore écrit seulement les stores présents et lisibles dans la sauvegarde ; il ne supprime pas un store absent. Redémarrez et vérifiez `/api/v1/status` et `/api/v1/library`. Les anciennes conclusions embarquées migrent vers `conclusions.json` lors de la prochaine sauvegarde de catalogue sous verrou ou d’une restauration version 1 ; leur simple lecture ne réécrit rien. Gardez une sauvegarde avant mise à jour/restauration. Sauvegardez séparément l’audio original.

<div id="docker-image-on-a-linux-host-procedure-only" data-legacy-anchor></div>

## Image Docker sur Linux — procédure seulement

Cette procédure indépendante du fabricant concerne une **future** image de conteneur Linux. Le dépôt ne construit, ne publie ni ne teste encore d’image Aède : `aede:local` ci-dessous est un exemple, **pas** une image téléchargeable aujourd’hui. Il suppose un exécutable compatible `/usr/local/bin/aede` dans l’image. Les versions Linux actuelles ciblent x86_64, pas aarch64 ; l’image doit correspondre au processeur hôte. Cette procédure n’exige aucun paquet NAS ou intégration de service spécifique.

Préparez des dossiers hôtes existants/persistants pour musique, données et sauvegardes. Remplacez chemins absolus/compte numérique par vos chemins et un UID/GID non administrateur pouvant parcourir/lire la musique et écrire exclusivement données/sauvegardes. Le dossier de données ne doit pas être ouvert en écriture au groupe/autres. Gardez `/music` et `/data` comme **mêmes chemins de conteneur** à chaque lancement : le catalogue enregistre les chemins, leur changement fait apparaître des pistes absentes. Un montage musical en lecture seule permet scan/consultation ; les fichiers annexes demandent au contraire un montage volontairement accessible en écriture.

```sh
AEDE_IMAGE='aede:local'                 # replace with a real, trusted image tag
AEDE_MUSIC='/absolute/path/to/music'   # already exists on the Docker host
AEDE_DATA='/absolute/path/to/aede-data'
AEDE_BACKUPS='/absolute/path/to/aede-backups'
AEDE_UID_GID='1000:1000'               # replace with the account that owns the directories

docker run --rm --user "$AEDE_UID_GID" \
  -e AEDE_HOME=/data \
  --mount "type=bind,src=$AEDE_MUSIC,dst=/music,readonly" \
  --mount "type=bind,src=$AEDE_DATA,dst=/data" \
  --entrypoint /usr/local/bin/aede "$AEDE_IMAGE" scan /music

docker run -d --name aede --restart unless-stopped \
  --stop-signal SIGTERM --stop-timeout 300 \
  --network host --user "$AEDE_UID_GID" -e AEDE_HOME=/data \
  --mount "type=bind,src=$AEDE_MUSIC,dst=/music,readonly" \
  --mount "type=bind,src=$AEDE_DATA,dst=/data" \
  --mount "type=bind,src=$AEDE_BACKUPS,dst=/backups" \
  --entrypoint /usr/local/bin/aede "$AEDE_IMAGE" serve
```

Sous Linux, `--network host` rend l’écoute `127.0.0.1:8787` accessible aux processus de l’hôte Docker, dont ceux partageant son espace réseau, pas aux autres machines. Ne remplacez pas cela par réseau bridge et `-p` : Aède écouterait toujours sur la boucle locale du conteneur, inaccessible par cette redirection. Le réseau hôte n’utilise pas `-p`. Vérifiez `http://127.0.0.1:8787/api/v1/status`, `/api/v1/library` sur l’hôte et `docker logs aede`. Ces commandes concernent Docker Engine Linux ; Docker Desktop et autres moteurs demandent une validation distincte. Voir la documentation Docker [réseau hôte](https://docs.docker.com/engine/network/drivers/host/) et [montages bind](https://docs.docker.com/engine/storage/bind-mounts/).

Exécutez les commandes CLI coopérantes **dans le conteneur actif**, pour partager compte, `/data` et espace du socket privé `/tmp` ; un autre conteneur avec seul montage des données ne partage pas ce socket. Par exemple `docker exec aede /usr/local/bin/aede stats` ou `docker exec aede /usr/local/bin/aede backup /backups/aede-backup.aede`. Copiez les sauvegardes hors hôte/NAS ; audio et fichiers annexes, absents du bundle, demandent leurs sauvegardes séparées. Aucun jeton administratif dans image/commande. Au besoin, suivez sa configuration protégée ci-dessus et limitez l’accès à l’environnement du conteneur. N’exposez pas HTTP à distance.

Avant de dépendre du déploiement, arrêtez/redémarrez avec `docker stop aede`, `docker start aede` et vérifiez le retour du même catalogue. Répétez une restauration avec bundle monté en lecture seule vers un **autre dossier vide**, et serveur arrêté pour une restauration de production. Testez ensuite redémarrage réel de l’hôte et sauvegarde/restauration hors hôte. `--stop-timeout 300` laisse du temps aux travaux acceptés, mais une tâche plus longue peut demander annulation ou délai supérieur. La [politique de redémarrage Docker](https://docs.docker.com/engine/containers/start-containers-automatically/) peut relancer un conteneur actif avant redémarrage hôte ; elle ne prouve pas la disponibilité des montages musique/données, à vérifier ensuite.

Le 26 septembre 2026, une répétition macOS jetable **sans conteneur** a vérifié scan, serveur local, sauvegarde, redémarrage propre et restauration dans un autre dossier ; la somme de contrôle audio source est restée identique. **Aucune image Docker, vie de conteneur, volume NAS, permission de compte, sauvegarde hors hôte ni redémarrage NAS n’a été validé.**

Ne publiez pas le port 8787, ne le redirigez pas via routeur et ne l’exposez pas par proxy/tunnel pour écouter sur téléphone. La boucle locale n’authentifie pas par utilisateur ; `/api/v1` révèle catalogue/chemins et le jeton administratif ne sécurise pas HTTP distant. Comptes, autorisations, transport distant chiffré et lecture audio sont futurs. Le serveur actuel ne répond pas encore au scénario d’écoute distante sur téléphone.
