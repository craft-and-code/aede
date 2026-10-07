
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

Le MD5 audio FLAC concerne l’audio décodé ; le comparer exige le décodage. La commande `aede check`, limitée au conteneur, ne le vérifie pas. La lecture le compare désormais pendant son décodage normal ; `aede analyze` ou les [rapports FlacCompagnon](imported-analyses.md) peuvent fournir séparément un résultat MD5 attribué à leur source.

CRC valide et MD5 décodé divergent parfois : ils interrogent des propriétés différentes. Une divergence demande enquête, sans prouver une histoire de modification précise. Les inférences spectrales sont encore une autre question.

<div id="flac-audio-md5-during-playback" data-legacy-anchor></div>

## MD5 audio FLAC pendant la lecture

`aede play` et la lecture PCM native vérifient le MD5 audio présent dans le bloc STREAMINFO du FLAC pendant le décodage progressif qui fournit le son. Le décodeur FLAC Symphonia déjà utilisé le calcule sur les échantillons entiers originaux, avant conversion en `f32`, réduction des canaux, gain, conversion de fréquence ou autre traitement DSP. Aucun décodage préalable supplémentaire du fichier entier n’est effectué ; musique et tags restent inchangés.

Ce contrôle de la source est indépendant du mode de lecture local. La lecture sans effets et la lecture avec DSP utilisent le chemin FLAC vérifié du décodeur flottant ; la lecture stricte utilise le même vérificateur dans le décodeur entier. Choisir une sortie audio ne désactive pas le contrôle : ffplay reçoit du PCM déjà décodé par Aède. Le décodage de secours FFmpeg est limité aux sources Opus/AAC/ALAC prises en charge ; il ne retente pas un FLAC après une erreur d’intégrité. L’admission stricte reste limitée au FLAC natif mono/stéréo sur 16/24 bits ; la lecture ordinaire vérifie également les sources Ogg FLAC prises en charge.

La somme porte sur tous les canaux et échantillons décodés ; une somme nulle signifie que sa valeur est inconnue, conformément à la [RFC 9639, section 8.2](https://www.rfc-editor.org/rfc/rfc9639.html#section-8.2). Elle peut révéler une incohérence audio même lorsque les contrôles du conteneur réussissent. Le MD5 sert ici à contrôler la cohérence, sans constituer une preuve cryptographique d’origine ou d’authenticité ; voir la [RFC 6151](https://www.rfc-editor.org/rfc/rfc6151.html).

Le statut `flac_md5_status` du décodeur distingue `Pending`, `Verified`, `NoSignature` et `Mismatch`. La vérification ne se termine que lorsque le décodage atteint la fin du fichier. Arrêter ou sauter le morceau avant cette fin laisse une somme présente en attente, sans verdict réussi sur le morceau entier. Un MD5 STREAMINFO nul autorise la lecture avec `NoSignature` ; il n’est pas présenté comme une somme vérifiée. Une divergence provoque une erreur de décodage, empêche la fin normale de lecture et abandonne tout nouveau résultat de mesure de sonie (LUFS) sur le morceau entier. La route PCM native signale `decode_failed` ; l’audio déjà transmis ne peut pas être rappelé.

Le déplacement progressif dans un morceau décode et écarte toujours le préfixe avant de lire la suite. Si ce décodeur atteint ensuite la fin du fichier, il a parcouru le morceau entier et peut vérifier son MD5, même si l’historique d’écoute reste incomplet. Vérification du décodage et écoute complète répondent à des questions différentes.

Ce statut d’exécution n’écrit aucun verdict d’intégrité ni aucune analyse dans `conclusions.json`. Les conclusions Aède ou FlacCompagnon existantes ne dispensent pas le décodage actuel de cette comparaison. Les autres codecs conservent leur comportement. Subsonic/OpenSubsonic transmet les octets encodés originaux sans ce décodage ; la vérification de l’audio décodé y appartient donc au client. La même limite concerne l’envoi des fichiers originaux aux lecteurs réseau. Le PCM WAV ne contient pas le MD5 audio STREAMINFO du FLAC : accepter ses échantillons entiers en lecture stricte ne permet pas de le déclarer vérifié par MD5.

Les sources FLAC natives et Ogg utilisent le même vérificateur des échantillons entiers. Le nombre de trames source et les déclarations de format sont contrôlés ; des trames natives manquantes, répétées ou réordonnées provoquent une erreur même sans somme stockée. Les canaux indépendants sur 32 bits sont pris en charge, mais la version du codec utilisée ne peut pas décoder le canal latéral de 33 bits d’une trame stéréo corrélée sur 32 bits, ni le nouvel encodage explicite sur 32 bits de l’en-tête de trame. Ces sources sont refusées plutôt que déclarées vérifiées. Pour la lecture FLAC native, les corps des métadonnées facultatives sont sautés sur le descripteur du fichier initialement ouvert ; seuls STREAMINFO et l’audio encodé sont présentés au démultiplexeur. Les grandes pochettes ne sont ni décodées ni soumises à une limite de taille pour la lecture. Au plus 65 536 en-têtes de métadonnées sont acceptés, y compris les blocs vides, pour borner le travail d’ouverture. Aucun octet source n’est réécrit.

Ce contrôle ne certifie pas tous les octets du conteneur. Le contenu des métadonnées facultatives, les données non audio finales et l’ensemble des pages ou des flux enchaînés Ogg ne sont pas couverts par le verdict MD5 décodé. Conserver le contrôle de conteneur séparé pour rechercher des erreurs de structure.

La somme s’accumule pendant le décodage puis est comparée à STREAMINFO lorsque la source entière atteint sa fin. Ce n’est pas une comparaison continue entre Aède et la sortie audio, et aucun signal de retour du DAC n’est haché. La sortie DSP diffère volontairement de la référence source. Les adaptateurs exacts, le comptage des trames et les contrôles d’erreur de sortie établissent des faits logiciels distincts ; une capture numérique est nécessaire pour valider l’identité des échantillons d’une route physique. Un MD5 source vérifié ne prouve pas que le mélangeur du système, le pilote, le récepteur ou le DAC a laissé l’audio transmis inchangé.

[check](cli/check.md) détaille syntaxe, fils de travail et verdicts ; [copy](cli/copy.md) décrit la relecture des destinations nouvellement écrites, aux règles différentes.
