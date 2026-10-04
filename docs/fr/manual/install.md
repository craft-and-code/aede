# Installer Aède

Aède est un exécutable de terminal nommé `aede` (`aede.exe` sous Windows). Il se lance dans Terminal, un terminal Linux ou PowerShell. Aucune application graphique ni service de fond n’est installé automatiquement. Le site décrit le dépôt actuel ; une ancienne version téléchargée peut proposer moins de commandes. Vérifier `aede --version` et l’aide intégrée pour comparer les versions.

## Choisir la distribution

Le processus de publication prépare les archives suivantes. Télécharger un fichier réellement publié depuis [GitHub Releases](https://github.com/craft-and-code/aede/releases) : une cible du processus ne prouve pas qu’une version correspondante soit déjà publiée. Choisir le processeur de votre ordinateur, pas celui d’un autre NAS.

| Système | Nom dans l’archive | Lecture |
| --- | --- | --- |
| macOS, Apple Silicon | `macOS-AppleSilicon` | Sortie CPAL native compatible, sinon ffplay |
| Linux, x86_64 | `Linux-x86_64` | Archive musl, ffplay nécessaire à la lecture |
| Windows, x64 | `Windows-x64` | Sortie native avec repli ffplay et touches console implémentées ; validation console/périphérique à faire ; délégation locale encore Unix seulement |

Les prochaines versions ne proposeront plus d’archive précompilée pour les Mac Intel. Aucune archive Linux ARM/Raspberry Pi ni image Docker publiée dans le processus actuel. Compiler une autre cible exige une validation spécifique. Le serveur propose une API de catalogue, la [lecture PCM native authentifiée](../server/playback.md) et la [diffusion du fichier original Subsonic/OpenSubsonic](../server/subsonic.md). L’accès distant exige la [configuration HTTPS](../server/remote.md) explicite ; aucun lecteur web/mobile ni déploiement NAS validé n’est fourni.

## Lancer un exécutable téléchargé

Extraire l’archive. Sous macOS/Linux, ouvrir un terminal dans ce dossier :

```sh
./aede --version
./aede help
./aede scan "$HOME/Music"
```

`./` signifie « lancer le fichier de ce dossier ». Ajouter ensuite ce dossier au `PATH` du terminal permet d’écrire `aede` depuis n’importe où. Ne pas placer l’exécutable ou ses données dans un dossier supprimé à chaque mise à jour.

Sous Windows, extraire le ZIP et ouvrir PowerShell dans ce dossier :

```powershell
.\aede.exe --version
.\aede.exe help
.\aede.exe scan "$env:USERPROFILE\Music"
```

Les archives ne sont pas signées. Vérifier le fichier SHA-256 associé : `shasum -a 256 ARCHIVE.tar.gz` sur macOS, `sha256sum ARCHIVE.tar.gz` sur Linux, `Get-FileHash ARCHIVE.zip -Algorithm SHA256` dans PowerShell, puis comparer avec la somme téléchargée. macOS peut mettre un téléchargement non signé en quarantaine. Après vérification de la source et de la somme, la procédure du projet utilise `xattr -dr com.apple.quarantine ./aede` sur cet exécutable précis ; ne pas retirer la quarantaine d’autres téléchargements.

## Compiler les sources actuelles

Choisir cette voie pour des fonctions plus récentes qu’une publication. Installer Git et Rust via [rustup](https://rustup.rs/). Rust 1.89 minimum. La compilation télécharge les dépendances, dont une version fixée de FlacCompagnon.

```sh
git clone https://github.com/craft-and-code/aede.git
cd aede
cargo build --release --locked -p aede-cli
./target/release/aede --version
```

Debian/Ubuntu GNU/Linux demande aussi les en-têtes ALSA et `pkg-config` pour la sortie native :

```sh
sudo apt install pkg-config libasound2-dev
```

Sous Windows, installer la chaîne Rust MSVC et les outils C++ Microsoft qu’elle requiert, puis lancer Cargo dans PowerShell. Résultat : `.\target\release\aede.exe`. Sur macOS, les outils de développement/édition de liens système sont nécessaires ; installer les outils en ligne de commande si l’éditeur de liens manque. La commande ci-dessus respecte le verrou de dépendances ; `tools/build.sh` est un flux développeur connecté qui peut actualiser FlacCompagnon et lancer tous les contrôles.

## Outils audio facultatifs

Scan, navigation, requêtes, annotations, sauvegarde et API de catalogue ne nécessitent pas FFmpeg. Installer `ffmpeg`/`ffplay` pour conversion, chemins de spectrogramme/décodage concernés et lecture de recours. Opus/AAC/ALAC peuvent nécessiter FFmpeg. Lire les tags d’un format ne garantit pas son décodage par le lecteur.

```sh
# macOS avec Homebrew
brew install ffmpeg
# Debian/Ubuntu
sudo apt install ffmpeg
```

Sous Windows, prendre une distribution depuis les liens de la [page FFmpeg](https://ffmpeg.org/download.html) et ajouter son dossier `bin` au `PATH`. Vérifier `ffmpeg -version` et `ffplay -version` dans le terminal d’Aède. Les empreintes demandent aussi `fpcalc` ou un FFmpeg avec Chromaprint ; une installation FFmpeg ordinaire ne garantit pas cette fonction. Les clés de services sont réservées aux passes `fetch` explicitement choisies.

## Choisir le dossier de données

Musique et données Aède sont distinctes. La musique est l’audio original. Les données contiennent `catalog.json`, `conclusions.json`, `user.json`, `sources.json` et fichiers dérivés. Ordre de sélection : `--data DOSSIER`, `AEDE_HOME`, `$XDG_DATA_HOME/aede`, puis `~/.local/share/aede` ; si `HOME` est absent, `.aede` dans le dossier courant. Cette résolution vaut sur tous les systèmes : choisir explicitement un dossier Windows évite de dépendre de variables de style Unix.

```sh
aede scan "$HOME/Music" --data "$HOME/aede-data"
aede stats --data "$HOME/aede-data"
```

```powershell
.\aede.exe scan "D:\Music" --data "$env:LOCALAPPDATA\Aede"
.\aede.exe stats --data "$env:LOCALAPPDATA\Aede"
```

Reprendre toujours le même emplacement pour CLI/serveur. `--data` ne signifie pas « scanner ce dossier musical ». Continuer avec [premiers pas](first-steps.md) et [sécurité des données](catalog.md).
