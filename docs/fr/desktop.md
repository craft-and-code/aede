# Application serveur Aède

Aède 0.6.0 propose deux distributions distinctes : la **CLI** existante et une **App** qui lance cette même CLI embarquée comme serveur HTTPS local. Cette application est une console serveur ; Phémios assure la navigation et la lecture. Les builds restent non signés. Les installateurs de l’App sont distincts des archives CLI ; compiler cette branche permet de tester les changements non encore publiés.

## Pourquoi cette application

Cette console rend le moteur existant accessible sans apprendre les commandes du Terminal : choisir les dossiers musicaux, démarrer un serveur local chiffré, vérifier son statut et consulter les journaux de requêtes. Elle facilite notamment les essais d’Aède et de Phémios comme deux applications distinctes sur le même ordinateur. Elle ne remplace ni le lecteur Phémios ni la CLI sans interface graphique destinée aux serveurs et NAS. Elle utilise son propre profil de données ; Phémios s’y connecte avec le profil public exporté et le compte indiqué dans la console.

## Démarrer

Choisissez le paquet **App** de votre système dans les [publications](https://github.com/craft-and-code/aede/releases). Les archives de la ligne de commande portent **CLI**. Ouvrez **Aède**, ajoutez les dossiers musicaux, choisissez un port libre (8787 par défaut), cliquez sur **Démarrer le serveur**, puis sur **Scanner les dossiers**. Aucun scan ni enrichissement réseau ne démarre implicitement. Le suivi, le résultat et l’arrêt du scan réutilisent les [tâches administratives](server/jobs.md).

Les dossiers sont modifiables même quand le serveur fonctionne, en dehors d’un scan actif dans l’App. Seul le changement de port impose son arrêt. Le scan applique la sélection courante non vide. Retirer un dossier ne supprime jamais ses fichiers ; retirer tous les dossiers configurés n’efface pas le catalogue conservé. Un dossier absent est conservé pendant un scan. Cette première application détecte les chemins disparus, mais pas encore un volume réseau démonté qui laisse un point de montage vide accessible : remontez-le avant le scan.

Une nouvelle sélection trop volumineuse pour la requête de scan est refusée avant son enregistrement, même en prévoyant l’absence de tous les volumes. La sélection précédente et l’identité de connexion sont conservées. Un ancien profil trop volumineux reste lisible. Retirez ses dossiers un par un jusqu’à obtenir une sélection admissible : chaque retrait strict est enregistré, même si d’autres restent nécessaires. Le scan demeure refusé tant que la sélection dépasse la limite ; ajouter ou remplacer des dossiers ne contourne pas cette validation.

Si un alias indisponible ne peut pas être relié de façon sûre aux chemins précédemment scannés, le scan refuse de publier un catalogue de remplacement. Reconnectez le volume ou sélectionnez son dossier réel, puis réessayez. Certaines modifications combinées, par exemple un alias indisponible inconnu et d’autres pistes disparues, nécessitent cette récupération explicite : le profil ne conserve pas d’historique des cibles d’alias.

Le serveur écoute uniquement sur `127.0.0.1`, en HTTPS. Un port occupé produit une erreur ; l’application n’adopte pas un autre serveur et ne se replie pas en HTTP. Pour un serveur distant ou LAN, utilisez la [configuration HTTPS de la CLI](server/remote.md).

## Relier Phémios sur le même ordinateur

1. Démarrez Aède et exportez sa connexion locale.
2. Dans Phémios : **Paramètres → Serveur Aède → Importer la connexion de l’application Aède**. Choisissez le JSON exporté.
3. Affichez le mot de passe généré dans Aède, puis saisissez-le dans Phémios avec le compte `aede-desktop` indiqué.
4. Après un scan, rechargez le catalogue connu dans Phémios.

L’export contient seulement l’adresse HTTPS exacte, le compte et le certificat public. Il ne contient aucun mot de passe, jeton de session ou clé TLS privée. Phémios accepte ce profil uniquement pour HTTPS `127.0.0.1` avec un port explicite et vérifie le certificat pour HTTPS **et WSS**. Importez uniquement la connexion d’une application Aède que vous souhaitez autoriser. Aucune installation de certificat système ni désactivation des vérifications TLS n’est nécessaire. Réimportez le profil après un changement de port ou dans une nouvelle session Phémios.

## Statut, journaux et accès au navigateur

Trois vues sont disponibles : **Serveur** pour les dossiers et scans, **Statut** pour vérifier explicitement l’API HTTPS authentifiée et le catalogue, et **Journaux** pour voir les requêtes et les événements du processus. Le statut distingue le processus actif, une API qui répond et un catalogue chargé, avec ses compteurs. Son actualisation ne lance aucun scan.

Les icônes de copie placent le login, l’adresse HTTPS active ou le mot de passe généré dans le presse-papiers natif. Copier le mot de passe ne le révèle pas à la webview et le marque comme sensible pour les gestionnaires de presse-papiers qui respectent cette indication ; l’icône afficher/masquer reste à côté. Aucune commande de lecture du presse-papiers n’est autorisée à la webview.

Ouvrir `/` dans le navigateur affiche une page statique expliquant comment connecter Phémios. Cette page n’est ni une console d’administration ni un lecteur ; elle ne révèle aucun chemin privé, compte ou contenu de bibliothèque. L’API reste authentifiée lorsqu’un compte est configuré. Le navigateur peut demander d’approuver le certificat local, tandis que Phémios vérifie explicitement le certificat importé.

L’App active `AEDE_REQUEST_LOG=1` pour son moteur. Un serveur CLI peut activer cette variable explicitement. Les résumés stderr `AEDE_HTTP` contiennent uniquement méthode HTTP connue, modèle de route enregistré (aucun identifiant de ressource ni paramètre d’URL), code de réponse et durée avant les en-têtes. Les rejets précoces et chemins inconnus ont une étiquette générique. La file d’écriture est bornée à 128 entrées pour ne pas bloquer les requêtes ; une saturation peut omettre des résumés. Ce journal n’est donc pas un registre d’audit ni une preuve de lecture complète d’un flux.

L’App lit stderr avec une limite par ligne et conserve uniquement les résumés validés et des messages fixes de cycle de vie ou de diagnostic. La sortie brute du moteur reste exclue ; le résultat détaillé d’un scan se consulte dans Serveur. Les 500 dernières entrées restent en mémoire, même après arrêt/redémarrage du serveur, jusqu’à leur effacement ou la fermeture de l’App. L’icône de téléchargement exporte les entrées conservées dans `aede-journaux.json`, avec un dialogue natif proposant le dossier Téléchargements. Seuls les résumés filtrés sont exportés, jamais stderr brut ni les accès du serveur. Aucun fichier de logs n’est créé automatiquement et aucun envoi distant n’a lieu.

Les gestes du trackpad ne provoquent ni navigation horizontale ni rebond élastique. Les zones avec ascenseur conservent leur défilement normal. Les textes et images de l’interface ne sont ni sélectionnables ni glissables ; les champs éditables gardent leur édition habituelle. Login, adresse et mot de passe ont leurs icônes de copie.

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

La mise à jour du moteur est distincte du scan des dossiers et de la synchronisation de Phémios : ces actions utilisent l’API du processus en cours et ne peuvent pas lui ajouter une fonctionnalité. Après l’installation ou la compilation d’une App Aède plus récente, redémarrer le serveur ou l’application, reconnecter Phémios si son ancienne session a expiré, puis synchroniser explicitement son catalogue. La vue Compositeurs nécessite la capacité `dynamic_browse_composers` dans ce moteur, même lorsque les tags des fichiers contiennent déjà les crédits.
