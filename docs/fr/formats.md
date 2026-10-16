
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

Le caractère sans perte d’un WavPack suit les propriétés déclarées du flux sélectionné, y compris les flux hybrides avec perte. Ces derniers n’affichent pas de profondeur PCM sans perte ; cela n’ajoute pas la gestion des fichiers de correction `.wvc`. Pour les formats secondaires, le tag principal (par exemple APEv2 dans WavPack) précède les variantes historiques comme ID3v1, dont les valeurs distinctes restent disponibles. Relire les métadonnées avec `scan --full` applique ces règles aux entrées déjà en cache.

Un WavPack flottant conserve son codec connu et son caractère sans perte, mais sa précision PCM reste inconnue car les propriétés partagées ne décrivent pas cette représentation. Le contrôle de représentation PCM examine au plus 64 en-têtes de blocs WavPack pour identifier le premier bloc audio ; une représentation non résolue laisse aussi la précision inconnue. La conversion en FLAC/WAV est alors refusée avant encodage. La copie à l’identique et une conversion explicite avec perte restent possibles ; cela n’ajoute pas la conversion des WavPack flottants.

<div id="parser-precision-and-resilience" data-legacy-anchor></div>

### Précision et robustesse des lecteurs

Les lecteurs dédiés gardent les détails utiles aux étapes suivantes : délai/remplissage LAME, pré-suppression Opus, disposition des canaux et informations de flux ALAC. Les fichiers de régression couvrent entrées tronquées/mal formées et fichiers réels produits par des outils audio. Un tag lu ne vérifie pas tous les échantillons ni l’intégrité : voir [contrôles d’intégrité](integrity.md) et [analyses acoustiques attribuées](imported-analyses.md).

Pour l’audio MPEG, le codec annoncé suit la couche lue dans le flux (`mp1`, `mp2` ou `mp3`), au lieu de supposer la couche III à partir du nom `.mp3`. Cela n’ajoute aucune extension et ne garantit pas un nouveau décodeur de lecture.

La désynchronisation ID3v2.3 est annulée sur le tag entier ; en ID3v2.4, elle l’est sur chaque trame après consommation de sa taille encodée, pour conserver la frontière des trames suivantes. L’identité de groupe et l’éventuel indicateur de longueur sont retirés avant le décodage du texte. Les jours/mois ID3 historiques `TDAT`/`TDA` sont conservés dans `recordingdaymonth`, séparément des valeurs `date` contenant une année.

Les clés MusicBrainz des champs personnalisés ID3 et MP4 sont normalisées une seule fois. L’identifiant de la piste dans une édition reste distinct de celui de l’enregistrement et ne crée ni ne divise les identités canoniques des enregistrements. Le champ de compatibilité `Track.mbid` continue de désigner l’enregistrement. Utiliser `scan --full` pour relire ces tags dans un cache de métadonnées existant.

Pour les anciens FLAC précédés d’un tag ID3v2, les commentaires Vorbis natifs sont prioritaires et les champs/valeurs ID3 distincts restent disponibles. Cette règle vaut pour la lecture par chemin comme par fichier déjà ouvert.

Les lecteurs de métadonnées, l’extraction des images intégrées et les vérificateurs natifs d’intégrité/résolution refusent les sources audio non ordinaires avant leur ouverture, notamment les FIFO déjà présentes ; les liens vers des fichiers ordinaires restent acceptés. Le descripteur ouvert est vérifié à nouveau, mais cette séquence métadonnées/ouverture n’empêche pas un remplacement concurrent du chemin.

Les propriétés MP4 proviennent des pistes audio, sans remplacement par une piste vidéo. Le WAV PCM extensible distingue les bits utiles de la largeur de stockage ; les codecs AIFC compressés non pris en charge n’ont pas de précision PCM ni de durée déduites.

Pour AAC dans MP4, les fréquences supérieures à 65535 Hz proviennent de la configuration du décodeur `esds`, lue avec des limites strictes, car elles ne tiennent pas dans le champ 16.16 de l’entrée audio. Les fréquences ordinaires conservent leur interprétation existante ; il ne s’agit pas d’un validateur complet des extensions HE-AAC ni d’un décodeur. Une configuration tronquée ou non prise en charge ne remplace pas la fréquence de l’entrée. Utiliser `scan --full` pour appliquer la fréquence corrigée aux métadonnées déjà en cache.

La recherche de durée Ogg suit les limites des pages complètes. Dans la fenêtre de fin, le CRC existant sert à reconnaître une page lors de la resynchronisation depuis une position inconnue ; cette lecture bornée n’est pas un contrôle d’intégrité du fichier entier et ne décode pas l’audio.

<div id="dependencies-and-playback" data-legacy-anchor></div>

## Dépendances et lecture

`lofty` lit les formats secondaires. La bibliothèque Rust FlacCompagnon fournit l’analyse en processus. L’utilisateur n’installe pas l’exécutable FlacCompagnon séparé pour `aede analyze`.

Le lecteur progressif actuel possède des chemins natifs testés pour FLAC, WAV PCM, MP3 LAME et Vorbis natif. Opus, AAC et ALAC dans M4A passent par un décodage FFmpeg facultatif. Cela ne promet pas un chemin de lecture pour tous les autres formats catalogués. [play](cli/play.md) détaille sélections et erreurs.

CPAL fournit une sortie native si compilation/système/périphérique le permettent ; sinon ffplay. Les archives Linux musl utilisent ffplay ; une compilation GNU Linux peut utiliser ALSA. FFmpeg intervient aussi dans `copy --compress` et certains chemins de spectrogramme/décodage. Les empreintes exigent `fpcalc` ou un FFmpeg contenant Chromaprint, non garanti dans une installation ordinaire.

<div id="why-preserve-decoder-details" data-legacy-anchor></div>

### Pourquoi garder les détails du décodage ?

Suppression du remplissage et bornes de trames soutiennent la continuité du traitement logiciel. Elles ne prouvent pas la continuité physique sur tous les périphériques. Jointures, vidange et performances NAS restent à valider. Le lecteur actuel ne doit pas être décrit comme universellement bit-perfect ou physiquement gapless.

Voir [installation](manual/install.md), [play](cli/play.md), [copy](cli/copy.md), [fingerprint](cli/fingerprint.md).
