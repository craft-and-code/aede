# play — Écouter la musique

play accepte un fichier, un dossier parcouru récursivement dans l’ordre, un M3U/M3U8, une collection ou un nom catalogué. collection:NOM désigne explicitement une collection. Les chemins relatifs d’une playlist partent de son dossier, pas du terminal. Fichiers/dossiers se lisent sans scan ; noms et collections demandent un catalogue.

Dans un terminal macOS/Linux : Espace met en pause/reprend, n ou Droite avance, p ou Gauche revient (ou recommence après trois secondes), q ou s arrête. Sans entrée terminal, la sélection avance automatiquement. Les touches de transport Windows restent à développer. Les écoutes sont enregistrées dans les données personnelles.

La normalisation album s’applique par défaut à un album catalogué ; les autres sélections utilisent track vers -18 LUFS. Les mesures actuelles FlacCompagnon, tags ou valeurs en cache peuvent fournir le gain. Une mesure manquante se calcule pendant la lecture, sans changer le niveau en cours de piste. Les hausses graves/aigus réservent une marge. CPAL fournit la sortie native compatible, sinon ffplay ; Opus/AAC/ALAC peuvent nécessiter ffmpeg. Les archives Linux exigent ffplay. Le garde de sortie signale les dépassements d’échantillon ; aucun limiteur dynamique ni plafond true peak garanti. La continuité physique sur matériel reste à mesurer.

AEDE_AUDIO_BACKEND choisit la sortie locale : absent, essayer native puis ffplay ; native exige la sortie native sans repli ; ffplay choisit explicitement ce programme. Toute autre valeur est refusée. Exemple macOS/Linux : `AEDE_AUDIO_BACKEND=ffplay aede play "/path/to/track.flac"`. Avec PowerShell, définir `$env:AEDE_AUDIO_BACKEND = "ffplay"` avant play. Cela choisit la sortie, pas le décodeur ni la qualité DSP.

## Syntaxe et arguments

```text
aede play <file|folder|m3u|collection|artist|album|track> [--normalize off|track|album] [--bass DB] [--treble DB]
```

Une sélection ; entourer de guillemets noms/chemins avec espaces. Préfixer une collection par collection:.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--normalize off\|track\|album` | Choisir off, track ou album pour la normalisation. Par défaut album pour un album du catalogue, track pour les autres sélections. |
| `--bass DB` | Réglage large des graves, de -12 à +12 dB. 0 est neutre ; les hausses réservent une marge. |
| `--treble DB` | Réglage large des aigus, de -12 à +12 dB. 0 est neutre ; les hausses réservent une marge. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede play "$HOME/Music/album/01.flac"
aede play "Kind of Blue" --normalize album
aede play collection:Road --bass 2 --treble -1
aede play "$HOME/Music/album/album.m3u" --normalize off
```

## Résultat et erreurs

Le terminal affiche album et nom de fichier numéroté, 24 barres spectrales animées, étapes DSP actives, marges de normalisation/correction et mesures de sortie. Le compteur observe le PCM protégé soumis à la sortie avant dither/conversion de périphérique, pas le son mesuré au haut-parleur. Lire la source de normalisation et les interventions du garde pour comprendre un changement de niveau. La fin naturelle, Suivant après la dernière piste et Arrêt rendent le terminal. Décodeur/sortie absent, disposition de canaux inconnue, playlist mal formée ou erreur de décodage/sortie produisent un diagnostic ; des écoutes terminées peuvent être déjà sauvées. Une sous-alimentation de périphérique est distincte d’un problème de tag. Le guide DSP précise les limites de mesure.

## Pour continuer

[track](track.md), [analyze](analyze.md), [history](history.md).

Guide détaillé existant : [design/playback.md](../../design/playback.md).
