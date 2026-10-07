# Régler les graves et les aigus

Aède possède deux **filtres en plateau** facultatifs : ils augmentent/réduisent une large zone de basses ou hautes fréquences, contrairement à une cloche paramétrique étroite. Graves et aigus sont des réglages d’écoute, pas une correction acoustique automatique.

## Régler et remettre à zéro

```sh
aede play "Un Album" --bass 3 --treble -2
aede play "Un Album" --bass -6 --treble 0
aede play "Un Album" --bass 0 --treble 0
```

Chaque valeur est un nombre fini entre −12 et +12 dB ; défaut zéro. Un nombre négatif fonctionne séparément ou avec `--bass=-6`. Positif augmente la zone, négatif la diminue. Zéro retire le filtre concerné ; deux zéros contournent **exactement les filtres de tonalité**. Normalisation, conversion de canaux/fréquence et protection peuvent cependant rester actives.

La lecture locale démarre sans effets. Une valeur de tonalité non nulle choisit DSP si `--playback` est omis ; elle est refusée avec without-effects ou bit-perfect explicite. En DSP, choisir `--normalize=off` pour appliquer seulement la tonalité, sans normalisation automatique de sonie.

Les points centraux habituels sont **120 Hz** pour les graves et **4 kHz** pour les aigus. Un centre situe la transition, pas une coupure brutale. Aux fréquences d’échantillonnage très faibles, ils sont abaissés sous Nyquist (moitié de cette fréquence). Fréquence/pente/Q ne sont pas réglables dans cette version. Les coefficients suivent les plateaux de l’Audio EQ Cookbook via `biquad`, avec un état par canal.

## Une hausse réduit aussi le préampli

Il faut de la place avant la pleine échelle. Aède réserve prudemment la somme des hausses : graves +6 dB et aigus +3 dB produisent **9 dB** d’atténuation préalable. Graves +3 et aigus −2 réservent 3 dB. Une baisse n’accorde pas un supplément de hausse. Cette réserve s’ajoute à la limite de crête de normalisation ; ce n’est pas un limiteur suivant les crêtes en permanence.

Le programme peut donc sembler globalement moins fort malgré le changement de tonalité. Ajustez le volume système au besoin ; ce n’est pas la preuve d’un filtre inactif. Transitoires et métadonnées de crête incorrectes peuvent dépasser cette estimation : la protection finale reste active et compte les interventions. Voir [Marge de niveau](headroom.md).

## Traitement continu

L’état des filtres survit aux transitions naturelles compatibles pour ne pas les remettre à zéro à chaque piste. Une interruption ou un flux incompatible réinitialise le traitement. Remplacer les réglages efface l’état ; la CLI les choisit au démarrage, sans éditeur de paramètres en direct avec transitions progressives.

La courbe animée illustre les plateaux et leur réserve sur un exemple synthétique. Ce n’est pas une mesure calibrée de votre pièce/casque ni du filtre Rust à chaque fréquence.

Égalisation paramétrique, profils de périphériques et convolution sont des [propositions Premium](premium.md), pas des options actuelles.
