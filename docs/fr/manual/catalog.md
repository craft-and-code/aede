# Catalogue et sécurité des données

Aède catalogue la musique locale ; il ne gère pas achats ou prêts de disques physiques. Le scan lit fichiers/tags sans réécrire les originaux. copy crée une destination distincte ; analyses, images, paroles et playlists peuvent créer des annexes explicitement demandées.

## Vocabulaire du graphe

| Objet | Sens | Exemple |
| --- | --- | --- |
| Release / album | Édition avec positions de pistes et fichiers locaux | Un pressage CD d’album |
| Release group | Identité d’album commune aux éditions | L’album avant le choix de pressage |
| Recording | Une interprétation enregistrée | La même performance sur original et compilation |
| Track | Placement d’un enregistrement dans une édition | Piste 3 du disque 1 |
| Work | Une composition | Œuvre interprétée dans plusieurs enregistrements |
| Artiste / crédit / relation | Personnes, rôles et liens attribués | Guitariste, producteur, compositeur, membre |

Un titre ne suffit pas à créer une identité commune. Identifiants MusicBrainz et liens explicites distinguent éditions, performances et parties classiques. Copier Open/Continue pour une navigation précise.

## Quatre magasins persistants

| Fichier | Contenu | Récupération |
| --- | --- | --- |
| `catalog.json` | Graphe des tags et dossiers suivis/exclus | Reconstruire par scan ; renommer les racines après reset |
| `conclusions.json` | Intégrité, empreintes, analyses importées/dérivées | Sauvegarder ; audio modifié = résultat obsolète |
| `user.json` | Notes, étoiles, favoris, étiquettes, écoutes, collections, choix | Travail personnel irremplaçable à sauvegarder |
| `sources.json` | Propositions externes attribuées et décisions | Sauvegarder ; certains faits peuvent être récupérés à nouveau |

Les faits externes restent à côté des tags avec provenance/confiance. Une identité proposée/en conflit exige review avant de devenir un lien de confiance. Le Markdown personnel reste intact ; son rendu relève du client, pas d’une réécriture du stockage.

## Droits et cohérence

Le dossier musical exige lecture pour scan/lecture. Le dossier Aède exige lecture/écriture pour mises à jour et ne doit pas être modifiable par des tiers en mode serveur. Une création d’annexe demande aussi écriture à destination. Le serveur exécute les commandes déléguées avec les droits de son compte.

Ne pas supprimer `.aede.lock`, éditer les JSON actifs ni mélanger anciens exécutables et serveur actuel. Tous les écrivains actuels partagent un verrou. Sur Unix, le serveur coordonne les CLI compatibles via sa socket privée ; sans lui, elles tournent localement. Lire un ancien instantané ne signifie pas qu’une mise à jour est perdue.

## Portée d’une sauvegarde

`aede backup FICHIER` garde les quatre magasins lisibles, historique inclus. Il ne copie pas musique, images, paroles ni playlists. Les sauvegarder séparément et garder une copie hors machine. `export` sert à inspecter/partager ; `rules` à reproduire des choix ; aucun ne remplace sauvegarde complète et originaux.

Préserver l’ancien état avant mise à jour/restauration. Répéter restore dans un dossier `--data` vide distinct. Le catalogue restauré est daté ; scan le rapproche des fichiers actuels. Voir [backup](../cli/backup.md), [restore](../cli/restore.md), [sources](../cli/sources.md), [review](../cli/review.md).
