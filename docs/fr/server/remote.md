# Accès distant chiffré

Par défaut, `aede serve` reste en HTTP sur la boucle locale de cet ordinateur. Une écoute HTTPS explicite permet aux clients authentifiés d’une autre machine de consulter le catalogue, leurs données personnelles et le [contrat audio](playback.md). Initialisez les [comptes](accounts.md) localement avant de l’activer. Le démarrage HTTPS refuse des comptes absents ou illisibles ; le jeton administratif historique ne remplace pas une session.

## Configurer HTTPS

Fournissez une chaîne de certificats PEM et sa clé privée PEM correspondante, non chiffrée. Le certificat doit couvrir le nom utilisé par les clients et être reconnu comme fiable par eux. Les deux chemins doivent être des fichiers ordinaires, sans liens symboliques ; chaque fichier est limité à 256 Kio et la chaîne à 16 certificats. Sous Unix, la clé privée doit interdire tout accès au groupe/aux autres, par exemple mode 0600. Aède ne délivre ni ne renouvelle les certificats, et ne recharge pas leur remplacement en cours d’exécution : redémarrez après modification. Sous Windows, protégez ces fichiers avec les ACL du compte du service ; Aède contrôle automatiquement les bits de permissions seulement sous Unix.

```sh
aede serve --bind 0.0.0.0 --port 8787 \
  --tls-cert /private/aede/fullchain.pem \
  --tls-key /private/aede/key.pem \
  --authority music.example:8787
```

Remplacez chemins et nom par votre configuration. `--bind` accepte une adresse IP littérale, IPv6 comprise : c’est l’adresse d’écoute. `--authority` est l’adresse publique exacte `HOST:PORT`, sans schéma, chemin, identifiants ni joker ; encadrez un nom IPv6 par des crochets. Son port peut différer du port d’écoute avec une redirection réseau explicite conservant TLS. La CLI refuse le port 0 avec TLS. Les trois options TLS sont obligatoires ensemble. Une écoute hors boucle locale sans TLS est refusée ; HTTPS peut aussi écouter sur la boucle locale.

Configurez séparément DNS, routage et pare-feu. Aède ne redirige pas automatiquement les ports et ne fait pas confiance aux en-têtes de proxy. TLS se termine dans Aède ; transférer du HTTP depuis un proxy terminant TLS vers une écoute HTTP Aède exposée n’est pas pris en charge. Vérifiez avec un client contrôlant le certificat et utilisant l’adresse configurée ; ne désactivez pas cette vérification.

## Authentifier les clients

La connexion native et les sessions Bearer suivent le même contrat que les [comptes locaux](accounts.md), avec `https://music.example:8787` et `wss://music.example:8787` à la place de HTTP et WS. Chaque requête native de catalogue ou de données personnelles exige une session valide. L’[adaptateur Subsonic/OpenSubsonic](subsonic.md) utilise séparément des clés d’application révocables pour ses opérations `/rest` authentifiées ; seule sa découverte d’extensions est publique. Le propriétaire vient de l’identifiant d’authentification, jamais d’un identifiant fourni par le client. Un auditeur lit le catalogue et ses vues personnelles ; lecture audio et autres mutations exigent un utilisateur ou administrateur.

Chaque requête porte un seul Host correspondant à l’adresse configurée. Une cible sous forme absolue utilise HTTPS et la même adresse. Une Origin de navigateur, si présente, doit correspondre exactement à cette origine HTTPS ; les clients natifs peuvent l’omettre. Les en-têtes de proxy ne changent pas ces vérifications. Aucune autorisation d’origine tierce, interface de connexion de navigateur ou session par cookie n’est implémentée. Les mots de passe natifs et jetons Bearer ne doivent jamais apparaître dans une URL. Les identifiants de l’adaptateur passent par les paramètres query/form du protocole ; masquez-les dans les journaux et préférez POST form quand méthode/client le permettent.

## L’administration reste locale

HTTPS désactive toute la famille `/api/admin` : administration des comptes, tâches de l’installation et routes personnelles transitoires de `local`. Cela vaut aussi pour une session administrateur et une écoute HTTPS sur boucle locale. Le jeton historique `AEDE_ADMIN_TOKEN` est indisponible en HTTPS. Utilisez `/api/me/v1` pour vos données ; gérez les comptes et lancez scan/fetch depuis la CLI locale de confiance. Le canal de commandes Unix privé garde sa portée locale.

## Limites de déploiement

Une réponse HTTP sans progrès d’écriture pendant dix secondes est déconnectée. L’arrêt laisse au plus quinze secondes aux connexions HTTPS établies, WebSockets compris. L’attente finale de cinq secondes des travailleurs de fond décrite dans le [guide d’exploitation](../operating.md) commence seulement après la fin des tâches/commandes locales acceptées.

Au plus 64 connexions TCP/TLS actives sont admises, WebSockets compris. Négociation TLS et en-têtes HTTP possèdent dix secondes ; les en-têtes sont limités à 64 champs et un tampon d’analyse de 32 Kio. Les requêtes disposent de deux travailleurs de fond bornés et d’un délai de réponse de quinze secondes ; un travail continuant après ce délai garde sa place jusqu’à sa fin. À saturation, une connexion ferme avant HTTP ; travail de requête occupé : `503 request_limit`, délai dépassé : `503 request_timeout`, échec du travailleur : `503 request_failed`. Réessayez les lectures après les autres opérations. Notifications et audio recontrôlent les sessions : déconnexion, révocation et modification des identifiants arrêtent l’accès sans attendre l’expiration. L’audio possède aussi quatre places de décodage et sa fenêtre de confirmation bornée.

Ce transport HTTPS sert le contrat natif et l’adaptateur Subsonic/OpenSubsonic implémenté, avec leurs authentifications et sémantiques audio distinctes. Aède ne fournit pas de lecteur mobile. Paquets NAS, mémoire sur bibliothèque réelle, performances sur cible et restitution physique demandent encore une validation de déploiement ; voir le [guide d’exploitation](../operating.md) et les [exigences Compatible Aède](compatible-aede.md).
