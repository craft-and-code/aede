# Activer l’administration locale

L’administration permet de lancer scan/fetch et de consulter/modifier favoris, étoiles, notes, historique et collections du propriétaire local. Elle est facultative et **désactivée par défaut**. Elle ne crée pas de comptes et n’autorise pas l’exposition distante du serveur.

## Configurer le jeton avant le démarrage

Choisissez un secret aléatoire ASCII privé d’au moins 32 caractères, placez-le dans `AEDE_ADMIN_TOKEN` dans l’environnement du serveur, puis démarrez Aède. Pour un service permanent, utilisez un secret protégé du gestionnaire de services. En session temporaire, vous pouvez saisir le secret sans l’afficher ni inscrire sa valeur dans l’historique :

```sh
read -r -s AEDE_ADMIN_TOKEN
export AEDE_ADMIN_TOKEN
aede serve
```

Saisissez le secret et appuyez sur Entrée pendant l’attente de `read`. Cet exemple suppose un terminal acceptant `read` silencieux, comme bash/zsh. Gardez cet environnement privé. Sans jeton, les routes d’administration ne sont pas enregistrées (404). Un jeton configuré invalide empêche une configuration de démarrage valide. Définir la variable seulement chez le client ne l’active pas dans un serveur déjà lancé : redémarrez avec l’environnement serveur configuré.

Dans un autre terminal client local de confiance, définissez le même secret de manière protégée. Chaque requête envoie **un seul** en-tête :

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/collections' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

La valeur est substituée localement. Ne placez jamais le secret dans une URL, du JavaScript public, un dépôt, une capture de journal ou un exemple publié. Tout en-tête Origin est refusé, même celui d’une page localhost correspondante. Ces routes concernent des clients locaux/natifs, pas un formulaire de navigateur. Les vérifications Host/Origin peuvent rejeter une requête avant l’authentification.

## Règles du corps de requête

Pour JSON, envoyez `Content-Type: application/json` et un objet, pas un tableau/texte. Les noms utilisent `snake_case` ; champs inconnus et types incorrects produisent `400 invalid_body`. Corps limité à 16 Kio, reçu en une seconde maximum (`408 request_timeout` sinon). Un objet valide avec combinaisons/plages invalides produit `400 invalid_parameters`. Les opérations sans corps ne doivent pas recevoir d’objet JSON. En particulier, **scan sans corps** et **scan avec `{}`** choisissent volontairement deux modes différents.

Les lectures n’ont pas de corps. Les routes de tâches refusent les paramètres ; les routes personnelles n’acceptent que les sélecteurs/pagination documentés. Les GET personnels/de tâches acceptent aussi HEAD avec les mêmes authentification/validation, sans corps de réponse.

## Erreurs et permissions

`401 unauthorized` signale un en-tête Bearer absent/incorrect/répété ou une Origin présente. `404 not_found` sur toutes les routes indique généralement une administration non activée au démarrage. Le jeton autorise les accès aux fichiers avec les permissions du **compte serveur** : dossiers de scan et cibles fetch sont des chemins de cet hôte. Ce n’est pas un environnement isolé pour appelant non fiable.

Le serveur ne change ni audio ni tags. Les images/paroles téléchargées et résultats d’analyse sont des fichiers dérivés distincts et peuvent demander des droits d’écriture dans leur destination. Les clés des services externes viennent de l’environnement serveur, jamais d’un remplacement HTTP. Les données personnelles appartiennent au seul propriétaire `local` existant ; la requête ne choisit pas d’autre utilisateur.

Suite : [Tâches scan/fetch et annulation](jobs.md) ou [Données personnelles](personal.md). Le [guide d’exploitation](../../operating.md) couvre les permissions de service et les sauvegardes.
