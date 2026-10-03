# Mono, stéréo et multicanal

Un **canal** est un signal destiné à une position de haut-parleur. Leur nombre ne dit pas lequel est avant-gauche, centre ou arrière-droit. Aède conserve un masque/agencement connu lorsque le décodeur le fournit et refuse d’inventer les positions d’un multicanal non pris en charge.

## Mélange de lecture local

Mono/stéréo conservent leur sens ordinaire. Les sources multicanales connues sont ramenées en stéréo avant gain/tonalité/protection communs. Les masques classiques reconnus couvrent 2.1, 3.0, 3.1, quad, 4.0, 5.0, 5.1, 7.0 et 7.1. Un nombre connu avec masque inconnu reste refusé.

Le **downmix** combine les canaux au lieu de supprimer tout sauf gauche/droite. Le centre contribue aux deux côtés ; les ambiances à leur côté. Les coefficients habituels centre/ambiance sont −3 dB (environ 0,707), inspirés d’ITU-R BS.775. En 7.1, côtés/arrière utilisent chacun −6 dB avant réduction finale. Chaque ligne stéréo est réduite prudemment pour qu’une somme de signaux cohérents à pleine échelle ne sature pas.

Exemple 5.1 : gauche reçoit avant-gauche + 0,707 centre + 0,707 ambiance-gauche, puis le coefficient prudent final ; droite fonctionne de même. La matrice réelle dépend du masque connu. Le niveau global peut diminuer en conservant le contenu de ces canaux. Ce n’est pas une recette universelle de mixage cinéma.

## Le LFE est omis

Le canal `.1` est celui des **effets basse fréquence**, pas toutes les basses de la musique. Aède l’omet dans le mélange stéréo actuel. Les basses présentes dans les canaux principaux restent. Aucune coupure de caisson, distribution de graves, temporisation ou réglage individuel de canal n’est actuellement proposé.

```sh
aede play /chemin/vers/fichier-5.1-reconnu.flac --normalize off
```

Pas d’option downmix nécessaire : le lecteur local choisit ce parcours stéréo. Désactiver la normalisation ne désactive pas le mélange. Un agencement inconnu/non accepté produit une erreur explicite plutôt qu’une position devinée.

## Limites de décodage et sortie

Le décodage de secours FFmpeg ne fournit actuellement pas de masque fiable au-delà de la stéréo : ce multicanal ne peut pas être mélangé par Aède ; mono/stéréo restent possibles. Format/agencement source sont conservés pour les diagnostics malgré une lecture stéréo. La [route PCM native](../server/playback.md) utilise le même parcours mono/stéréo et le même mélange des canaux connus ; la transmission du PCM multicanal original n’est pas implémentée. [Subsonic/OpenSubsonic](../server/subsonic.md) transfère le fichier encodé original : son client gère le décodage et le mélange des canaux.

La matrice n’ajoute pas de latence de traitement et n’alloue rien par bloc ; les buffers système/périphérique sont une autre question. Le modèle interactif illustre canaux connus et LFE omis, sans détecter vos enceintes ni transmettre de multicanal.

Voir [Choix de sortie](output.md) et [futures idées de canaux/gestion des graves](premium.md).
