# Adapter la fréquence au périphérique

La **fréquence d’échantillonnage** est le nombre de trames PCM par seconde, par exemple 44100 Hz (44,1 kHz). Ce n’est ni le débit du fichier ni la profondeur entière du DAC. La changer demande de reconstruire/filtrer le signal puis de le rééchantillonner ; retirer ou répéter des échantillons produirait des erreurs.

## Choix natif automatique

Le lecteur natif choisit format, canaux et fréquence compatibles. Il préfère les formats flottants avant de comparer les distances de fréquence : un flottant à autre fréquence peut donc passer avant un entier plus proche. Consultez fréquences source/sortie affichées ; un fichier 96 kHz ne signifie pas que le périphérique reçoit 96 kHz.

```sh
aede play /chemin/vers/morceau-96k.flac
```

Aucun sélecteur de fréquence de sortie n’existe actuellement en CLI. À fréquence identique, Aède contourne son convertisseur. Sinon, le décodage utilise une conversion à **bande limitée** avec état, avant gain/tonalité/protection. Le secours ffplay reçoit la fréquence décodée ; une conversion ultérieure de ffplay/système échappe aux diagnostics du convertisseur Aède.

## Pourquoi filtrer

Nyquist vaut la moitié de la fréquence. Au-delà du nouveau Nyquist, le signal ne peut plus être représenté et peut se replier en fréquences basses fausses (**aliasing**). Une baisse de fréquence filtre donc les aigus avant conversion. Une hausse ajoute des positions d’échantillons ; elle ne restaure pas de détails absents ni ne transforme une compression avec pertes en sans pertes.

Aède utilise la conversion FFT Rubato pour les couples fixes habituels et sinc pour les rapports inhabituels. L’état survit aux blocs. Le retard initial du filtre est retiré ; sa fin est vidée au terme d’un flux ininterrompu. Les pistes compatibles partagent état et limites cumulatives de trames, sans arrondi/réinitialisation indépendant à chaque fichier. Sauts et formats incompatibles abandonnent l’état en attente.

## Validation et limites

Les tests logiciels couvrent une grille documentée de fréquences/signaux : erreur de bande passante au plus 0,1 dB jusqu’à 80 % du Nyquist inférieur, rejet résiduel/repliement au moins 80 dB et retard de phase résiduel au plus une trame. La transition haute au-delà de cette bande est examinée séparément. Ce sont des résultats de tests, pas une promesse pour chaque signal, DAC, latence ou processeur NAS. La [revue DSP](../../coding/dsp-review.md#reference-signal-validation) détaille périmètre et limites.

L’exemple interactif montre positions d’échantillons et besoin de filtrage, sans mesurer les performances du convertisseur Rust. Buffers/transitions physiques restent à mesurer sur matériel ; voir [Continuité](continuity.md).
