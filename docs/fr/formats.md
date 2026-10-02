
<div id="formats-and-optional-tools" data-legacy-anchor></div>

# Formats et outils facultatifs

Aède distingue la lecture des tags/conteneurs du décodage audio pour écouter ou analyser. Un format catalogué n’est pas automatiquement un format de lecture native. Les fichiers originaux et leurs tags restent en lecture seule.

<div id="supported-formats-reading-the-library" data-legacy-anchor></div>

## Formats pris en charge : lire la bibliothèque

Les lecteurs principaux prennent en charge :

| Conteneur | Codecs | Tags | Origine de la durée |
| --- | --- | --- | --- |
| FLAC | FLAC | Vorbis Comment, ID3v2 initial | STREAMINFO |
| MP3 | MPEG 1/2/2.5 couches I–III | ID3v2.2/2.3/2.4, ID3v1 | Xing / VBRI / débit constant |
| MP4 | ALAC, AAC | Atomes iTunes, champs libres `----` | `mvhd` |
| Ogg | Vorbis, Opus | Vorbis Comment | Position des granules |
| WAV | PCM | `LIST/INFO`, bloc `id3 ` | `fmt ` + `data` |
| AIFF / AIFC | PCM | `NAME`/`AUTH`, bloc `ID3 ` | `COMM` |

Extensions : `.flac`, `.mp3`, `.m4a`, `.m4b`, `.mp4`, `.alac`, `.ogg`, `.oga`, `.opus`, `.wav`, `.wave`, `.aif`, `.aiff`, `.aifc`.

Les autres formats reconnus passent par `lofty` lorsqu’aucun lecteur principal ne correspond :

| Conteneur | Codecs | Tags | Origine de la durée |
| --- | --- | --- | --- |
| AAC | AAC | ID3v2, ID3v1 | En-têtes ADTS |
| WavPack | WavPack | APEv2, ID3v1 | En-têtes de blocs |
| Monkey’s Audio | APE | APEv2, ID3v1 | Descripteur |
| Musepack | Musepack SV7/SV8 | APEv2, ID3v1 | En-tête du flux |
| Speex | Speex | Vorbis Comment | Position des granules |

Extensions : `.aac`, `.ape`, `.wv`, `.mpc`, `.mp+`, `.mpp`, `.spx`.

<div id="parser-precision-and-resilience" data-legacy-anchor></div>

### Précision et robustesse des lecteurs

Les lecteurs dédiés gardent les détails utiles aux étapes suivantes : délai/remplissage LAME, pré-suppression Opus, disposition des canaux et informations de flux ALAC. Les fichiers de régression couvrent entrées tronquées/mal formées et fichiers réels produits par des outils audio. Un tag lu ne vérifie pas tous les échantillons ni l’intégrité : voir [contrôles d’intégrité](integrity.md) et [analyses acoustiques attribuées](imported-analyses.md).

<div id="dependencies-and-playback" data-legacy-anchor></div>

## Dépendances et lecture

`lofty` lit les formats secondaires. La bibliothèque Rust FlacCompagnon fournit l’analyse en processus. L’utilisateur n’installe pas l’exécutable FlacCompagnon séparé pour `aede analyze`.

Le lecteur progressif actuel possède des chemins natifs testés pour FLAC, WAV PCM, MP3 LAME et Vorbis natif. Opus, AAC et ALAC dans M4A passent par un décodage FFmpeg facultatif. Cela ne promet pas un chemin de lecture pour tous les autres formats catalogués. [play](cli/play.md) détaille sélections et erreurs.

CPAL fournit une sortie native si compilation/système/périphérique le permettent ; sinon ffplay. Les archives Linux musl utilisent ffplay ; une compilation GNU Linux peut utiliser ALSA. FFmpeg intervient aussi dans `copy --compress` et certains chemins de spectrogramme/décodage. Les empreintes exigent `fpcalc` ou un FFmpeg contenant Chromaprint, non garanti dans une installation ordinaire.

<div id="why-preserve-decoder-details" data-legacy-anchor></div>

### Pourquoi garder les détails du décodage ?

Suppression du remplissage et bornes de trames soutiennent la continuité du traitement logiciel. Elles ne prouvent pas la continuité physique sur tous les périphériques. Jointures, vidange et performances NAS restent à valider. Le lecteur actuel ne doit pas être décrit comme universellement bit-perfect ou physiquement gapless.

Voir [installation](manual/install.md), [play](cli/play.md), [copy](cli/copy.md), [fingerprint](cli/fingerprint.md).
