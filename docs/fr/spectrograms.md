<div id="spectrograms-the-ultimate-acoustic-truth" data-legacy-anchor></div>

# Spectrogrammes : la réalité acoustique en images

Un spectrogramme est un outil de diagnostic visuel lorsque l’origine d’un fichier est incertaine. Les métadonnées peuvent être incorrectes ou trompeuses. Un fichier FLAC provenant d’une conversion de MP3 peut toujours indiquer un codec ou un conteneur sans perte, tandis que son spectre apporte des indices supplémentaires sur l’histoire de son audio. Une coupure spectrale autour de 16 kHz ne prouve pas, à elle seule, une conversion depuis un format avec perte : le matériau d’origine et des traitements antérieurs peuvent également limiter la bande passante. Ni un spectrogramme ni les tags seuls ne garantissent une origine sans perte ou une provenance complète.

Aède fournit une vue détaillée du signal que vous archivez.

```sh
aede spectrum                       # reveal the acoustic truth of the entire catalog
aede spectrum ~/Music/Ozzy          # focus the microscope on a specific shelf
aede spectrum --dry-run             # preview the effort without drawing a single pixel
aede spectrum --full                # fiercely redraw everything, overriding current files
aede spectrum --size full           # match FlacCompagnon's exact dimensions for direct comparison
```

<div id="the-physical-toll-of-acoustic-analysis" data-legacy-anchor></div>

## Le coût du traitement acoustique

**Sans dossier indiqué, Aède analyse toute la bibliothèque.** Pour une grande collection, le calcul représente un travail important. Chaque piste doit être entièrement décodée, puis une transformée de Fourier rapide (FFT) est appliquée au fil de l’audio. À plusieurs secondes par piste, visualiser des dizaines de milliers de morceaux peut prendre des heures, quel que soit le nombre de cœurs utilisés.

Ce coût justifie de commencer par `--dry-run` pour évaluer le périmètre exact de l’opération. Indiquer un dossier, par exemple `~/Music/Ozzy`, permet de réserver ce travail aux nouvelles extractions ou aux fichiers suspects.

Arrêter une longue exécution en cours ne supprime pas le travail déjà terminé. À l’exécution suivante, Aède reprend ce qui reste à produire et préserve les images déjà dessinées.

<div id="visual-provenance-and-flaccompagnon" data-legacy-anchor></div>

## Cohérence visuelle avec FlacCompagnon

Chaque image est enregistrée en PNG dans un sous-dossier `spectrograms/`, à côté des fichiers audio qu’elle décrit.

Les images sont produites avec **le même filtre FFmpeg et la même palette que [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/)**. Ce choix facilite l’utilisation des deux outils : des spectrogrammes utilisant des gains ou des palettes différents seraient difficiles à lire _ensemble_. La même méthode permet une représentation acoustique cohérente entre les outils.

<div id="frame-size-and-storage-footprint" data-legacy-anchor></div>

## Dimensions et espace de stockage

Vous choisissez les dimensions de l’image :

- **`--size half` (par défaut) :** produit une image de `900x470`, soit exactement un quart des pixels de l’image FlacCompagnon d’origine, de `1800x940`. Ces dimensions réduites peuvent économiser de l’espace, mais la taille du PNG et l’économie obtenue dépendent du contenu de l’image et de sa compression. La taille du fichier n’est pas garantie à un quart de celle de l’original ; une archive de milliers de pistes n’est pas non plus garantie de rester à l’échelle des mégaoctets plutôt que des gigaoctets.
- **`--size full` :** reprend les dimensions exactes de FlacCompagnon pour une comparaison côte à côte à la même échelle, pixel par pixel.

Changer `--size` ne redessine pas automatiquement les images existantes. Une image n’est recréée que si elle manque ou est périmée. Pour imposer un changement de taille à toute la bibliothèque, utilisez `aede spectrum --full`.

<div id="orchestration-and-dependencies" data-legacy-anchor></div>

## Organisation et dépendances

Aède traite plusieurs pistes en parallèle. Chaque image nécessite son propre décodage et son propre calcul FFT, sans état partagé avec les autres images. `--threads` fixe le nombre de traitements simultanés, selon la même logique que `aede scan`.

Aède organise le traitement, mais s’appuie sur un moteur externe pour décoder l’audio. **FFmpeg doit être installé** sur votre système : `brew install ffmpeg` sur macOS ou `sudo apt install ffmpeg` sur Debian/Ubuntu. Aède vérifie cette dépendance une seule fois, avant le premier fichier. Si elle manque, une seule notification est affichée, au lieu d’une erreur répétée pour chaque piste.

<div id="the-second-run-silent-and-safe" data-legacy-anchor></div>

## Une deuxième exécution sans travail inutile

**Une deuxième exécution sur une bibliothèque inchangée ne dessine rien.**

Une image est recréée uniquement si elle manque entièrement ou si la date de modification de la piste audio est postérieure à la date de création de l’image. Cette vérification lit directement les dates sur le disque, pas dans le catalogue. Elle sert à déterminer si le visuel doit être actualisé pour correspondre au fichier actuellement présent.
