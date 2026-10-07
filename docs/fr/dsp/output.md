# Sortie, indicateurs et diagnostics

Le son entendu inclut traitement Aède, sortie, système et périphérique. Les diagnostics identifient ce qu’Aède observe, sans qualifier tout le parcours de transparent ou bit-perfect.

## Sortie native et secours

`aede play` ordinaire utilise CPAL natif lorsqu’un format/canaux compatibles existent, sinon ffplay. Sans effets est le défaut local et privilégie la fréquence source ; DSP conserve la priorité au flottant. Bit-perfect strict utilise le backend ALSA direct distinct sous Linux glibc et exige un appareil matériel explicitement admissible ; les autres routes actuelles sont refusées. Opus et M4A peuvent demander FFmpeg en secours de décodage. Pour choisir explicitement une sortie ordinaire :

```sh
AEDE_AUDIO_BACKEND=native aede play /chemin/vers/morceau.flac
AEDE_AUDIO_BACKEND=ffplay aede play /chemin/vers/morceau.flac
```

`native` exige ce parcours sans secours silencieux ; ffplay doit être installé pour l’autre. Aède lui transmet des blocs traités `f32le`. La sortie native reçoit les données dans un tampon préalloué aligné par trames ; le callback Aède n’alloue, ne verrouille ni ne journalise. Pénuries et erreurs hôte restent possibles. L’historique est écrit de manière asynchrone pour ne pas bloquer le décodage.

`aede play --list-devices` affiche noms et identifiants natifs sans démarrer la lecture. Un `--output-device` explicite désactive remplacement/repli automatique ; le mode strict refuse aussi ffplay imposé. Voir [les modes de lecture](../cli/play.md#playback-policies-and-strict-output) pour l’admission et les limites des archives Linux statiques. Un appareil listé ou une admission logicielle réussie ne prouve pas l’identité numérique physique.

ALSA direct possède des tests de simulation logicielle et d’API avec configuration Linux simulée ; compilation native Linux, édition des liens, fonctionnement réel et capture numérique restent non vérifiés. Ces contrôles doivent réussir sur la plateforme visée avant de présenter sa sortie matérielle comme validée.

## Lire les étapes

Repérez mode de lecture, appareil/backend et format, fréquences/canaux source et sortie, source/gain de normalisation, marges de sonie/tonalité et conversions nécessaires. Sans effets signale adaptations et conversions entières réduisant la précision au lieu de revendiquer une sortie stricte. Strict distingue préservation source et capture physique non mesurée. Une étape désactivée diffère d’une étape indisponible. La réduction dynamique reste indisponible sans limiteur ; la CLI n’invente pas ce compteur à partir du garde.

## Mesure de crêtes en sortie

Le compteur observe les trames complètes de PCM protégé transmises à la sortie. Il donne nombre de trames, crêtes d’échantillons avant/après protection, estimation suréchantillonnée de crête vraie si disponible et nombre de valeurs modifiées par la protection. Une amplitude 1 vaut 0 dBFS ; en dessous, on reste sous pleine échelle. Une crête vraie inconnue n’est pas zéro.

À 192 kHz et au-delà, la crête vraie est inconnue faute de suréchantillonnage ; un échec de mesure peut aussi l’indisponibiliser. Des statistiques de segment incomplet sont explicitement indisponibles, pas prétendues complètes. Mesures avant dither/conversion du périphérique, sans mesure du son physique. Les [LUFS source](measurements.md) sont observés séparément avant traitement.

## Pénuries, erreurs et vidange

Compteurs de trames consommées, occupation bornée, pénuries de file et xruns hôte distinguent données décodées en retard et problèmes du pilote. Une **pénurie** signifie que la sortie demande des échantillons pas encore arrivés ; du silence exact peut être transmis. Une notification récupérable de route/ordonnancement n’est pas toujours fatale, mais reste un avertissement visible.

La sortie stricte traite les interruptions du programme, changements de route et erreurs de sortie inconnues comme des échecs définitifs de préservation, sans reprise automatique. Son historique compte les trames source consommées et exige une fin normale réussie de la source avant complétion. Historique ordinaire soumis/temps actif et confirmations du client distant restent des preuves séparées ; aucun n’est une confirmation physique du DAC.

Vidange finale et changements de format gardent les commandes utilisables. La vidange native considère cinq secondes sans progression consommée comme une erreur. CPAL ordinaire ajoute une tolérance hôte d’environ 100 ms en temps actif, hors pause ; ALSA direct utilise le retard matériel déclaré et une vidange non bloquante réussie. Aucun ne prouve physiquement qu’un échantillon est sorti du DAC. Ces limites ne certifient ni latence, ni jonctions parfaites, ni performances NAS.

En cas d’échec, conservez le message, vérifiez dépendances/formats/canaux et comparez avec tonalité plate et normalisation off. Ne supprimez ni originaux ni données pour réparer une erreur de périphérique. [Continuité](continuity.md) distingue jonctions PCM testées et sortie physique non mesurée.
