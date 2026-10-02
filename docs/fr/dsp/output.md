# Sortie, indicateurs et diagnostics

Le son entendu inclut traitement Aède, sortie, système et périphérique. Les diagnostics identifient ce qu’Aède observe, sans qualifier tout le parcours de transparent ou bit-perfect.

## Sortie native et secours

`aede play` utilise CPAL natif lorsqu’un format/canaux compatibles existent, avec priorité au flottant. Sinon il utilise ffplay. Opus et M4A demandent aussi FFmpeg pour le décodage. Pour choisir explicitement :

```sh
AEDE_AUDIO_BACKEND=native aede play /chemin/vers/morceau.flac
AEDE_AUDIO_BACKEND=ffplay aede play /chemin/vers/morceau.flac
```

`native` exige ce parcours sans secours silencieux ; ffplay doit être installé pour l’autre. Aède lui transmet des blocs traités `f32le`. La sortie native reçoit les données dans un tampon préalloué aligné par trames ; le callback Aède n’alloue, ne verrouille ni ne journalise. Pénuries et erreurs hôte restent possibles. L’historique est écrit de manière asynchrone pour ne pas bloquer le décodage.

## Lire les étapes

Repérez fréquence/canaux source et sortie, backend/format, source et gain de normalisation, sa marge, tonalité/marge éventuelles et conversion de fréquence active. Une étape désactivée diffère d’une étape indisponible dans ce format/parcours. La CLI indique la réduction dynamique indisponible sans limiteur ; elle n’invente pas ce compteur à partir de la protection dure.

## Mesure de crêtes en sortie

Le compteur observe les trames complètes de PCM protégé transmises à la sortie. Il donne nombre de trames, crêtes d’échantillons avant/après protection, estimation suréchantillonnée de crête vraie si disponible et nombre de valeurs modifiées par la protection. Une amplitude 1 vaut 0 dBFS ; en dessous, on reste sous pleine échelle. Une crête vraie inconnue n’est pas zéro.

À 192 kHz et au-delà, la crête vraie est inconnue faute de suréchantillonnage ; un échec de mesure peut aussi l’indisponibiliser. Des statistiques de segment incomplet sont explicitement indisponibles, pas prétendues complètes. Mesures avant dither/conversion du périphérique, sans mesure du son physique. Les [LUFS source](measurements.md) sont observés séparément avant traitement.

## Pénuries, erreurs et vidange

Compteurs de trames consommées, occupation bornée, pénuries de file et xruns hôte distinguent données décodées en retard et problèmes du pilote. Une **pénurie** signifie que la sortie demande des échantillons pas encore arrivés ; du silence exact peut être transmis. Une notification récupérable de route/ordonnancement n’est pas toujours fatale, mais reste un avertissement visible.

Vidange finale et changements de format gardent les commandes utilisables. La vidange native considère cinq secondes sans progression consommée comme une erreur. La tolérance hôte d’environ 100 ms compte en temps actif, hors pause ; ce n’est pas une confirmation physique de lecture. Ces limites ne certifient ni latence, ni jonctions parfaites, ni performances NAS.

En cas d’échec, conservez le message, vérifiez dépendances/formats/canaux et comparez avec tonalité plate et normalisation off. Ne supprimez ni originaux ni données pour réparer une erreur de périphérique. [Continuité](continuity.md) distingue jonctions PCM testées et sortie physique non mesurée.
