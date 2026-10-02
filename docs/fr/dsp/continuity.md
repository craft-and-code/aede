# Décodage continu et transitions d’album

Une transition peut contenir un silence voulu ou de la musique continue. **Gapless** signifie ne pas ajouter/retirer d’audio involontaire à la jonction, pas supprimer tout silence des fichiers. Les codecs peuvent introduire retard/remplissage à interpréter avant de joindre les trames jouables.

## Ce qui est implémenté

Le décodeur progressif fournit des trames PCM finies et complètes sans lire toute la sélection à l’avance. Des fichiers réels vérifient les nombres de trames jouables FLAC, WAV, MP3 LAME et Vorbis natif. Il retire retard/remplissage MP3 déclarés et limites de pré-saut/fin Opus. FFmpeg facultatif décode Opus et AAC/ALAC en M4A. Limites corrompues/tronquées, réinitialisations de chaînes Vorbis détectées et fins non établies produisent une erreur, pas un nombre de trames inventé.

Les formats de sortie compatibles réutilisent le flux natif ou le processus ffplay. Une session commune conserve tonalité et conversion compatibles aux jonctions naturelles, arrondit les limites cumulatives et attribue les échantillons retardés au bon morceau/gain. La fin du convertisseur n’est vidée qu’au terme du groupe. Un test réel FLAC-vers-MP3 vérifie le PCM exactement concaténé, sans trames insérées.

## Quand recommencer une session

Une incompatibilité d’entrée/sortie/tonalité relance la session. Une modification du format de sortie rouvre le périphérique. Saut, Previous, Stop et source en erreur abandonnent l’état en attente pour ne pas envoyer l’ancien audio à la nouvelle sélection. Une transition naturelle compatible diffère donc d’un saut demandé.

```sh
aede play "Un Album Continu" --normalize album
```

Sélectionnez un album catalogué pour conserver ordre et normalisation album. Sur terminal Unix, Espace pause/reprend, `n`/Droite avance, `p`/Gauche revient ou recommence après trois secondes, `q` arrête. Les contrôles de terminal Windows sont futurs. La CLI n’utilise pas encore les possibilités repeat/shuffle de la file en mémoire distincte.

## Frontières du matériel

Le tampon natif contient au plus 500 ms à la fréquence négociée, pas une réserve illimitée. Une ouverture/décodage trop lent peut l’épuiser et produire du silence de pénurie. Buffers du périphérique, ordonnanceur et réouverture peuvent influencer la jonction audible. **Le gapless physique n’a pas été mesuré sur matériel**, y compris les transitions traitées. La continuité PCM logicielle ne garantit pas celle de tout périphérique.

La vidange finale observe la consommation de trames et les commandes, avec une tolérance hôte approximative en temps actif. Ce n’est pas un accusé physique de sortie du dernier échantillon du DAC. Voir [Diagnostics](output.md) et [contrat technique des transitions/vidanges](../../design/playback.md).

L’animation illustre jonction, retrait de remplissage et réinitialisation ; elle ne lit ni ne vérifie un album réel.
