
<div id="are-the-files-still-intact" data-legacy-anchor></div>

# Les fichiers sont-ils encore intacts ?

`aede check` vérifie les sommes des conteneurs compatibles. Il lit les fichiers catalogués sélectionnés sans les modifier ni demander une copie de référence. C’est un contrôle de conteneur, pas une analyse acoustique complète.

| Conteneur | Contrôle |
| --- | --- |
| FLAC | CRC-16 des trames et CRC-8 de leurs en-têtes |
| Ogg (Vorbis, Opus, Speex) | CRC-32 des pages |
| MP3, MP4, WAV, AIFF et autres contrôles non pris en charge | Pas de somme compatible avec cette commande |

L’absence de contrôle ne dit pas qu’un format ne peut jamais contenir de somme ni que le fichier est prouvé sain. Elle décrit ce qu’Aède vérifie actuellement.

Le rapport distingue :

- **Non vérifié** : aucun verdict actuel établi.
- **Sans somme** : aucun contrôle de conteneur pris en charge.
- **Intact** : les contrôles compatibles correspondent.
- **Endommagé** : un contrôle a échoué ; lire trame/page et raison.

Les fichiers illisibles sont signalés séparément et ne reçoivent pas de verdict sain. Le traitement actuel peut finir avec code 0 malgré fichiers illisibles ou verdicts endommagés. Lire le rapport ; le code seul n’est pas un certificat. `doctor` signale aussi dégâts mémorisés et fichiers non vérifiés.

La vérification exige le flux complet. Le lecteur actuel refuse plus de 2 Gio de trames audio FLAC après les métadonnées, ou plus de 2 Gio pour un fichier Ogg entier. Ces fichiers sont signalés en erreur de lecture, sans nouveau verdict : vérifier un préfixe ne permet jamais de déclarer le fichier entier intact. Un fichier dont la taille change pendant la lecture est également refusé. Une relecture échouée retire l’ancien verdict de ce fichier, afin que le contrôle ordinaire suivant le retente ; empreintes et analyses sont conservées. Les anciennes versions pouvaient enregistrer un verdict portant seulement sur le préfixe à la limite de lecture. Les verdicts existants restent réutilisés jusqu’à une relecture explicite : lancer `check --full` sur les dossiers contenant de gros fichiers vérifiés par une ancienne version.

<div id="reuse-and-current-scope" data-legacy-anchor></div>

## Réutilisation et portée

Les verdicts sont dans `conclusions.json`, rattachés aux fichiers inchangés. Le scan ordinaire les garde ; un fichier modifié rend son ancien verdict inéligible. check normal saute les verdicts actuels mais affiche le rapport de toute la portée. `--full` les relit volontairement.

```sh
aede check
aede check "/path/to/music/album" --full
```

Les dossiers limitent aux fichiers catalogués sous leur chemin. Scanner la musique nouvelle auparavant. Un CRC intact ne prouve ni absence de réencodage, ni identité à un original séparé, ni conservation future sur le disque.

<div id="the-physical-toll-how-long-it-takes-and-how-to-start-small" data-legacy-anchor></div>

## Durée et première vérification

Le débit de stockage et la taille dominent la durée. Aucun délai fixe garanti sur NAS/disque. Commencer par un album puis élargir.

Les lots terminés sont enregistrés tous les 250 fichiers et à la fin. Ctrl-C sur un contrôle local perd au plus le lot en cours ; le suivant réutilise les verdicts actuels. Cela préserve le travail terminé, pas une protection contre panne de stockage. Sauvegarder conclusions et originaux séparément.

Sur Unix avec serveur correspondant, check est délégué. Fermer la CLI ou son Ctrl-C coupe l’affichage sans arrêter le contrôle accepté. `cancel` ne gère que scan/fetch délégués. L’arrêt gracieux du serveur attend les commandes, check compris. Voir [exploitation](../operating.md).

<div id="the-limits-of-the-container" data-legacy-anchor></div>

### Limites du conteneur

Le MD5 audio FLAC concerne l’audio décodé ; le comparer exige le décodage. Ni lecture actuelle ni check ne le vérifient. `analyze` ou [rapports FlacCompagnon](imported-analyses.md) peuvent apporter ce résultat attribué.

CRC valide et MD5 décodé divergent parfois : ils interrogent des propriétés différentes. Une divergence demande enquête, sans prouver une histoire de modification précise. Les inférences spectrales sont encore une autre question.

[check](cli/check.md) détaille syntaxe, fils de travail et verdicts ; [copy](cli/copy.md) décrit la relecture des destinations nouvellement écrites, aux règles différentes.
