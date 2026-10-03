# Activer l’administration locale

En HTTP local, l’administration permet de lancer scan/fetch et de gérer les données du propriétaire local. Un [compte administrateur](accounts.md) utilise sa session ; le jeton historique facultatif fonctionne aussi. Sans comptes ni jeton, l’administration est désactivée. [HTTPS](remote.md) désactive toutes les routes `/api/admin` et le jeton historique ; utilisez la CLI locale de confiance pour les tâches de l’installation.

## Configurer le jeton avant le démarrage

Choisissez un secret aléatoire ASCII privé d’au moins 32 caractères, placez-le dans `AEDE_ADMIN_TOKEN` dans l’environnement du serveur, puis démarrez Aède. Pour un service permanent, utilisez un secret protégé du gestionnaire de services. En session temporaire, vous pouvez saisir le secret sans l’afficher ni inscrire sa valeur dans l’historique :

```sh
read -r -s AEDE_ADMIN_TOKEN
export AEDE_ADMIN_TOKEN
aede serve
```

Saisir le secret puis Entrée. Cet exemple suppose bash/zsh et un environnement privé. Sans comptes, l’absence de jeton rend l’administration indisponible (404) ; un jeton configuré invalide empêche le démarrage. Définir la variable seulement chez le client ne change pas le serveur actif : redémarrer avec son environnement configuré.

Dans un autre terminal client local de confiance, définissez le même secret de manière protégée. Chaque requête envoie **un seul** en-tête :

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/collections' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

La valeur est substituée localement. Aucun secret dans une URL, du JavaScript public, un dépôt ou journal. Le jeton historique refuse toute Origin ; les sessions respectent la vérification d’origine locale commune. Aucune interface de connexion/cookies actuellement. Host/Origin peuvent être rejetés avant l’authentification.

## Règles du corps de requête

Pour JSON, envoyez `Content-Type: application/json` et un objet, pas un tableau/texte. Les noms utilisent `snake_case` ; champs inconnus et types incorrects produisent `400 invalid_body`. Corps limité à 16 Kio, reçu en une seconde maximum (`408 request_timeout` sinon). Un objet valide avec combinaisons/plages invalides produit `400 invalid_parameters`. Les opérations sans corps ne doivent pas recevoir d’objet JSON. En particulier, **scan sans corps** et **scan avec `{}`** choisissent volontairement deux modes différents.

Les lectures n’ont pas de corps. Les routes de tâches refusent les paramètres ; les routes personnelles n’acceptent que les sélecteurs/pagination documentés. Les GET personnels/de tâches acceptent aussi HEAD avec les mêmes authentification/validation, sans corps de réponse.

## Erreurs et permissions

`401 unauthorized` signale un Bearer absent/incorrect/répété, ou une Origin avec le jeton historique. Un compte `user` ou `auditor` reçoit `403 forbidden`. Sans comptes ni jeton, l’administration répond `404 not_found`. Les tâches utilisent les permissions du compte système serveur : dossiers scan/fetch sont des chemins de cet hôte et exigent un administrateur de confiance.

Le serveur ne change ni audio ni tags. Images/paroles et résultats d’analyse sont des fichiers dérivés pouvant demander des droits d’écriture. Les clés des services viennent de l’environnement serveur, jamais de HTTP. Les routes personnelles administratives gardent `local` ; `/api/me/v1` utilise le propriétaire de la session. Aucune requête ne peut choisir un autre propriétaire.

Suite : [Tâches scan/fetch et annulation](jobs.md) ou [Données personnelles](personal.md). Le [guide d’exploitation](../../operating.md) couvre les permissions de service et les sauvegardes.
