# Application serveur Aède

Aède 0.5.0 introduit deux distributions distinctes : la **CLI** existante et une **App** qui lance cette même CLI embarquée comme serveur HTTPS local. Cette application est une console serveur ; Phémios assure la navigation et la lecture. Les builds restent non signés. Tant que la version 0.5.0 n’est pas publiée, il faut compiler cette branche ; la publication précédente ne contient pas l’App.

## Démarrer

Choisissez le paquet **App** de votre système dans les [publications](https://github.com/craft-and-code/aede/releases). Les archives de la ligne de commande portent **CLI**. Ouvrez **Aède**, ajoutez les dossiers musicaux, choisissez un port libre (8787 par défaut), cliquez sur **Démarrer le serveur**, puis sur **Scanner les dossiers**. Aucun scan ni enrichissement réseau ne démarre implicitement. Le suivi, le résultat et l’arrêt du scan réutilisent les [tâches administratives](server/jobs.md).

Les dossiers et le port sont modifiables lorsque le serveur est arrêté. Le scan applique la sélection courante non vide. Retirer un dossier ne supprime jamais ses fichiers ; retirer tous les dossiers configurés n’efface pas le catalogue conservé. Un dossier absent est conservé pendant un scan. Cette première application détecte les chemins disparus, mais pas encore un volume réseau démonté qui laisse un point de montage vide accessible : remontez-le avant le scan.

Le serveur écoute uniquement sur `127.0.0.1`, en HTTPS. Un port occupé produit une erreur ; l’application n’adopte pas un autre serveur et ne se replie pas en HTTP. Pour un serveur distant ou LAN, utilisez la [configuration HTTPS de la CLI](server/remote.md).

## Relier Phémios sur le même ordinateur

1. Démarrez Aède et exportez sa connexion locale.
2. Dans Phémios : **Paramètres → Serveur Aède → Importer la connexion de l’application Aède**. Choisissez le JSON exporté.
3. Affichez le mot de passe généré dans Aède, puis saisissez-le dans Phémios avec le compte `aede-desktop` indiqué.
4. Après un scan, rechargez le catalogue connu dans Phémios.

L’export contient seulement l’adresse HTTPS exacte, le compte et le certificat public. Il ne contient aucun mot de passe, jeton de session ou clé TLS privée. Phémios accepte ce profil uniquement pour HTTPS `127.0.0.1` avec un port explicite et vérifie le certificat pour HTTPS **et WSS**. Importez uniquement la connexion d’une application Aède que vous souhaitez autoriser. Aucune installation de certificat système ni désactivation des vérifications TLS n’est nécessaire. Réimportez le profil après un changement de port ou dans une nouvelle session Phémios.

## Données et fermeture

Le profil est isolé dans les données d’application du système (`fr.craftandcode.aede`), sous `server/`. Aucun catalogue CLI ou moteur Phémios existant n’est adopté silencieusement. Un verrou exclusif empêche deux lanceurs de modifier le même profil. `server.json` conserve les dossiers, le port, le secret administrateur et l’identité TLS : dossier 0700 et fichier 0600 sous Unix, ACL utilisateur héritées sous Windows. Les secrets ne figurent ni dans les arguments des processus ni dans l’état public. Un profil corrompu, trop volumineux ou exposé est refusé, sans réinitialisation.

Le compte créé est un administrateur Aède normal. Ses droits et ses modifications restent gérés par le moteur. Si son mot de passe est modifié ailleurs, le lanceur refuse l’ancien secret sans réinitialiser le compte. L’administration avancée reste accessible dans Phémios ou la CLI.

L’application possède uniquement son processus enfant, via le [bail desktop](../server/desktop.md). Arrêter le serveur ou quitter l’application ferme ce bail, attend brièvement puis termine cet enfant si nécessaire. Aucun service de fond n’est installé, aucun pare-feu n’est modifié. Les travaux déjà enregistrés sont conservés ; un scan interrompu n’est pas annoncé comme terminé.

## Développement

Le workspace [`desktop/`](../../desktop/README.md) est indépendant : SolidJS, TypeScript strict, Vite, Tauri 2, Prettier, Oxlint et Vitest, comme Phémios. Le cœur Rust indépendant est dans `desktop/core`, le pont natif dans `desktop/src-tauri`. La CLI conserve Rust 1.89 ; le desktop nécessite Rust 1.90 ou ultérieur.

```sh
cd desktop
npm ci
cargo fetch --locked
npm run tauri dev
```

La préparation embarque la CLI du dépôt avec `--locked --offline`. Récupérez une fois ses dépendances depuis la racine si nécessaire. `AEDE_DEV_PORT` impose éventuellement le port Vite ; sans cette variable, le lanceur découvre un port disponible avant d’ouvrir la webview. `AEDE_DESKTOP_BINARY` sélectionne un moteur déjà compilé dont la version doit correspondre à l’App.

Dans `desktop/`, `bash tools/check.sh` vérifie le front, le pont et le code natif ; `npm run build:native` construit le paquet de la machine. La vérification racine couvre le moteur. GitHub prépare les distributions CLI et App séparées dans une publication brouillon après leurs vérifications natives. La configuration de CI ne remplace pas les essais manuels sur chaque système.
