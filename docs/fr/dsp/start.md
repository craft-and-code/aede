# Comprendre le traitement audio d’Aède

DSP signifie **traitement numérique du signal** : des calculs sur le son pendant la lecture. Le DSP actuel change le flux de lecture, jamais le fichier original ni ses tags. Il ne remplace pas le volume du système. La commande locale `aede play` et la route PCM native authentifiée partagent le traitement.

## Suivre un échantillon jusqu’à la sortie

1. Le décodeur transforme l’audio du fichier en PCM : une suite de nombres appelés échantillons. Une trame contient un échantillon par canal ; en stéréo, gauche et droite avancent ensemble.
2. Un agencement multicanal connu peut être ramené en stéréo, sans LFE, avec une réduction prudente du niveau.
3. La sortie native choisit format/fréquence compatibles ; au besoin, un convertisseur à bande limitée change la fréquence en conservant son état.
4. Une décision fixe de normalisation par piste/album ajuste le niveau, dans la limite permise par les crêtes.
5. Les réglages larges de graves/aigus appliquent leur correction, avec une marge supplémentaire en cas de hausse.
6. Une protection finale borne les valeurs dépassant la pleine échelle et compte ses interventions.
7. Les indicateurs observent le PCM transmis ; la sortie l’envoie au périphérique. En entier natif, un dither TPDF accompagne la quantification ; en flottant, cette conversion est contournée.

Cet ordre décrit la lecture locale. La capture du volume manquant observe le **PCM source avant traitement**, pour ne pas mesurer son propre gain/égalisation comme source. Les mesures de sortie voient le PCM protégé transmis **avant** dither/conversion du périphérique. Elles ne mesurent pas vos enceintes.

La [route PCM native](../server/playback.md) décode également la source, ramène les canaux connus en stéréo, convertit la fréquence au besoin et applique normalisation, tonalité/marge et protection finale. Son client gère le périphérique audio et sa conversion finale. Cette route transmet une seule piste : elle réutilise les données de volume existantes sans apprendre de nouvelles mesures ni enchaîner les pistes distantes. [Subsonic/OpenSubsonic](../server/subsonic.md) transfère le fichier audio encodé original sans DSP serveur ; son client gère le décodage et ses éventuels traitements de lecture.

## Les réglages actuels

```sh
aede play "Un Album"
aede play "Un Album" --normalize album --bass 3 --treble -2
aede play /chemin/vers/morceau.flac --normalize off --bass 0 --treble 0
```

Remplacez noms/chemins par les vôtres. Les albums du catalogue utilisent par défaut la normalisation album ; fichiers, dossiers, playlists, collections, artistes et pistes utilisent la normalisation piste. `--normalize off|track|album` change ce choix. Graves/aigus acceptent chacun −12 à +12 dB, avec zéro par défaut. Zéro contourne exactement les filtres ; normalize off seul **ne désactive pas** la tonalité ni la protection finale.

La CLI ne propose actuellement ni cible LUFS réglable, ni éditeur paramétrique, ni limiteur, ni profil de convolution, ni fréquence manuelle, ni interrupteur de dither. Ce ne sont pas des options cachées à deviner. La [référence play](../cli/play.md) couvre sélections et options ; les [diagnostics de sortie](output.md) montrent ce qui a réellement été exécuté.

## Choisir votre question

| Question | Guide |
| --- | --- |
| Pourquoi deux albums ont-ils des niveaux différents ? | [Normalisation](normalization.md) et [mesures](measurements.md). |
| Comment ajuster graves ou aigus ? | [Tonalité](tone.md) et [marge de niveau](headroom.md). |
| Que devient un fichier 5.1 ? | [Canaux](channels.md). |
| Pourquoi la sortie passe-t-elle de 96 à 48 kHz ? | [Conversion de fréquence](resampling.md). |
| Les échantillons entiers sont-ils transmis intacts ? | [Dither et sortie entière](dither.md). |
| Les transitions d’album sont-elles gapless ? | [Lecture continue](continuity.md). |
| Que signifient les barres animées ? | [Spectre](spectrum.md). |
| Quels effets Premium pourrait-il ajouter ? | [Futur Premium](premium.md). |

Les diagrammes interactifs du site utilisent des signaux synthétiques pédagogiques. Ce ne sont ni des mesures de votre matériel ni des commandes du DSP audio. La validation technique figure dans la [revue DSP](../../coding/dsp-review.md) et la [conception de lecture](../../design/playback.md).
