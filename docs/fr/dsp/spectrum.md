# Lire le spectre à 24 bandes

Les barres de `aede play` sont un **affichage de fréquences**, pas un égaliseur, une mesure LUFS ou une vérification d’intégrité. Les graves sont à gauche, les aigus à droite. L’affichage observe les échantillons sans modifier l’audio transmis.

```sh
aede play /chemin/vers/morceau.flac
```

## Construction de l’affichage

L’analyseur rassemble 2048 trames, applique une fenêtre Hann pour réduire les effets de bord fréquentiels et calcule une FFT à entrée réelle. Les crêtes sont groupées en **24 bandes logarithmiques**, d’environ 45 Hz jusqu’au minimum de 16 kHz et Nyquist. L’échelle logarithmique laisse une place lisible aux graves au lieu de la donner surtout aux aigus.

Le mono est observé directement. Pour plusieurs canaux, le mélange d’affichage pondère le premier à 75 % et la moyenne des autres à 25 %, afin qu’une stéréo exactement en opposition de phase ne disparaisse pas entièrement des barres. Ce mélange visuel n’est pas le downmix audio. L’analyseur lit seulement le PCM ; le spectre ne change donc pas les canaux joués.

Les niveaux montent plus vite qu’ils ne descendent et sont ramenés à des valeurs visuelles de 0 à 1. Ce ne sont ni des LUFS calibrés, ni des crêtes vraies, ni une énergie linéaire. Cymbale et note de basse peuvent exciter plusieurs bandes. Des barres faibles ne prouvent ni absence de haute résolution ni fichier endommagé.

## Terminal et limites

Le CLI regroupe les 24 bandes d’analyse en douze bandes larges et segmentées pour l’affichage Rétro, des basses aux hautes fréquences. Leur largeur suit celle du Terminal ; un Terminal étroit regroupe davantage les bandes au lieu de faire déborder l’affichage. Les colonnes se remplissent depuis le bas, avec des segments verts en bas, jaunes plus haut et rouges au sommet. Des repères de crête distincts retombent plus lentement après la baisse du niveau courant. Ces couleurs indiquent une hauteur d’affichage, pas des seuils d’écrêtage calibrés.

L’animation demande une sortie Terminal interactive. `NO_COLOR` ou `--no-color` conserve les blocs et repères de crête en monochrome ; `--lyrics` remplace le spectre par les passages de paroles. Une sortie redirigée ne constitue pas un flux d’animation audio. Le libellé montre album et nom de fichier numéroté sans chemin complet. La vue approche le contenu entrant dans la sortie choisie ; elle ne mesure ni mixeur système, ni DAC, ni pièce.

Pour crêtes/protection, consultez les [diagnostics de sortie](output.md). Pour une image du fichier dans le temps, la commande CLI `spectrum` produit des spectrogrammes avec le traitement d’analyse ; elle diffère de l’affichage Rétro en direct. Sa [référence CLI](../cli/spectrum.md) décrit size/threads/full.

Le spectre animé du site est synthétique pour montrer les bandes sans envoyer votre musique ni accéder au périphérique. Il n’analyse pas un fichier glissé dans la documentation.
