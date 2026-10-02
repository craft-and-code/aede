# Résoudre les problèmes

Lire l’erreur entière : Aède refuse arguments/options inutilisables plutôt que les ignorer. La plupart des corrections préservent les originaux. `aede help COMMANDE` montre les options de votre exécutable.

## Commande introuvable

Depuis le dossier extrait, `./aede` sur macOS/Linux ou `.\aede.exe` dans PowerShell. Sinon ajouter ce dossier au `PATH` et ouvrir un nouveau terminal. `PATH` trouve les programmes ; `--data` trouve le catalogue. Ce sont deux réglages.

## Aucun catalogue / bibliothèque vide

`aede scan "/chemin/musique"` avec le même `--data` que la commande en erreur. `--data` ne scanne jamais. Vérifier `roots`, `stats` et les droits de lecture. Citer les chemins avec espaces. Ne pas supprimer les vraies données personnelles pour résoudre un mauvais dossier de données.

## Disque suivi débranché

Le reconnecter avant scan. Aède refuse plutôt que perdre toutes les pistes absentes. Pour cesser volontairement son suivi : `roots --remove DOSSIER` ; lire le comportement de scan immédiat et utiliser `--no-scan` si adapté. reset supprime les racines ; il faudra les nommer à nouveau.

## Option/nom refusé

`albums`/`artists` listent ; `album TITRE`/`artist NOM` ouvrent. Mettre le titre avant l’option de nom : `track "So What" --artist "Miles Davis"`. Choisir un format d’export ; `--separator` et export `--tracks` exigent `--csv`. `--limit=0` est invalide ; utiliser `--all` si disponible. Une entité de requête inconnue produit une erreur, distincte d’une requête valide sans piste correspondante.

## Écriture de copie/téléchargement impossible

Vérifier espace libre et droits. copy refuse une destination chevauchant la bibliothèque. Les téléchargements d’annexes exigent des liens physiques pour publier sans écrasement ; FAT/exFAT ne convient pas. Ne pas contourner cela en écrasant les originaux. `--dry-run` prévoit mais peut encore vérifier les outils nécessaires.

## Métadonnées absentes après fetch

Choisir la passe : paroles `--lyrics`, discographie `--discography`, crédits `--credits`. `sources --list`, `review`, `credits` distinguent attente/non-fiable/vide/non-identifié. Les réponses terminées sont réutilisées ; `--full` les redemande volontairement. Les clés doivent être présentes dans le processus réel, serveur compris si délégation. Ne pas les copier dans un signalement.

## Lecture : outil/périphérique

Vérifier FFmpeg/ffplay dans le `PATH` du même terminal. Archives Linux : ffplay requis. Lire les tags ne garantit pas un décodeur natif. Les dispositions multicanal inconnues sont refusées ; celles connues peuvent devenir stéréo sans LFE. Les touches Windows ne sont pas développées. Interruptions/sous-alimentation peuvent rester audibles malgré le décodage correct ; la continuité physique n’est pas mesurée sur tous les périphériques. Lire les étapes DSP actives avant d’attribuer un faible niveau au fichier.

## Ctrl-C n’a pas arrêté scan/fetch

Si le serveur Unix accepte la tâche, fermer la CLI ne l’annule pas. `cancel TASK_ID` avec les mêmes données ; les réponses fetch terminées restent présentes. Les tâches HTTP utilisent la route administrative. Après redémarrage, les identifiants en mémoire expirent.

## Récupérer ou signaler

Garder les données et sauvegarder avant récupération. Arrêter le serveur pour restaurer réellement ; essayer dans un dossier distinct. Signaler version, système, commande/erreur expurgées et reproduction minimale sûre. Éviter chemins privés, notes, clés ou fichiers personnels complets. Voir [sécurité des données](catalog.md), [backup](../cli/backup.md).
