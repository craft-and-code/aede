# play — Écouter la musique

play accepte un fichier, un dossier parcouru récursivement dans l’ordre, un M3U/M3U8, une collection ou un nom catalogué. collection:NOM désigne explicitement une collection. Les chemins relatifs d’une playlist partent de son dossier, pas du terminal. Fichiers/dossiers se lisent sans scan ; noms et collections demandent un catalogue.

Dans un terminal macOS/Linux, les touches ci-dessous fonctionnent sans Entrée. Sans entrée terminal, la sélection avance automatiquement avec les modes de répétition et de mélange demandés. Ces options fonctionnent aussi sur Windows ; ses touches interactives restent à développer. Les écoutes sont enregistrées dans les données personnelles.

La normalisation album s’applique par défaut à un album catalogué ; les autres sélections utilisent track vers -18 LUFS. Les mesures actuelles FlacCompagnon, tags ou valeurs en cache peuvent fournir le gain. Une mesure manquante se calcule pendant la lecture, sans changer le niveau en cours de piste. Les hausses graves/aigus réservent une marge. CPAL fournit la sortie native compatible, sinon ffplay ; Opus/AAC/ALAC peuvent nécessiter ffmpeg. Les archives Linux exigent ffplay. Le garde de sortie signale les dépassements d’échantillon ; aucun limiteur dynamique ni plafond true peak garanti. La continuité physique sur matériel reste à mesurer.

AEDE_AUDIO_BACKEND choisit la sortie locale : absent, essayer native puis ffplay ; native exige la sortie native sans repli ; ffplay choisit explicitement ce programme. Toute autre valeur est refusée. Exemple macOS/Linux : `AEDE_AUDIO_BACKEND=ffplay aede play "/path/to/track.flac"`. Avec PowerShell, définir `$env:AEDE_AUDIO_BACKEND = "ffplay"` avant play. Cela choisit la sortie, pas le décodeur ni la qualité DSP.

## Syntaxe et arguments

```text
aede play <file|folder|m3u|collection|artist|album|track> [--seek TIME] [--repeat off|one|all] [--shuffle off|random|smart] [--seed U64] [--normalize off|track|album] [--bass DB] [--treble DB]
```

Une sélection ; entourer de guillemets noms/chemins avec espaces. Préfixer une collection par collection:.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--seek TIME` | Commencer la première occurrence lue à une position en secondes, `mm:ss` ou `hh:mm:ss`, avec jusqu’à trois décimales. Zéro par défaut. |
| `--repeat off\|one\|all` | Arrêter en fin de sélection, répéter le morceau ou répéter toute la sélection. off par défaut. |
| `--shuffle off\|random\|smart` | Garder l’ordre de sélection, mélanger uniformément ou privilégier des transitions progressives entre genres. off par défaut. Le mode smart demande un catalogue. |
| `--seed U64` | Reproduire un ordre mélangé avec une graine entière non signée sur 64 bits. Demande random ou smart. Sinon, une graine est générée et affichée. |
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
aede play "Kind of Blue" --seek 02:15.500
aede play collection:Road --shuffle random --repeat all
aede play collection:Journey --shuffle smart --seed 42
```

## Commandes du Terminal

| Touche | Action |
| --- | --- |
| Espace | Pause/reprise. |
| `n` ou Droite | Occurrence suivante, même avec la répétition du morceau. |
| `p` ou Gauche | Occurrence précédente, ou recommencer le morceau après trois secondes. |
| `[` / `]` | Reculer/avancer de dix secondes dans le morceau. |
| `r` | Faire défiler la répétition : off → one → all → off. |
| `z` | Faire défiler le mélange : off → random → smart → off. Sans catalogue, smart est sauté avec une explication. |
| `q`, `s` ou Ctrl-C | Arrêter, sauvegarder l’historique et revenir au shell. |

Répétition et mélange se changent aussi pendant une pause. Un changement de mélange conserve l’occurrence actuelle et celles déjà passées ; seules les occurrences restantes sont réordonnées. Désactiver le mélange restaure leur ordre relatif initial. Cela ne coupe pas le morceau en cours. Précédent suit l’ordre réellement joué, y compris le cycle précédent de répétition complète lorsqu’il est encore conservé.

## Se déplacer dans un morceau

`--seek` s’applique à la première occurrence effectivement jouée, y compris la première d’un ordre mélangé. Les morceaux suivants et les répétitions commencent à zéro. Reculer avant le début ramène à zéro ; avancer au-delà de la fin termine l’occurrence et suit le mode de répétition actif. Le déplacement conserve la pause.

Le décodeur rouvre la source puis décode progressivement le début pour l’écarter, avec une mémoire de travail bornée. Cette partie sautée ne passe ni dans le DSP, ni dans la sortie, ni dans l’historique. La position est arrondie vers le bas à une trame source ; son affichage utilise les millisecondes. Le déplacement n’est pas instantané par index : son coût augmente avec la position visée, et une lecture lente du fichier ou du décodeur peut retarder une commande entre deux contrôles d’annulation. Le déplacement réinitialise l’audio en attente et l’état DSP.

Plusieurs déplacements pendant une même visite créent une seule écoute incomplète. Sa durée estime l’audio soumis pendant le temps actif, en excluant pauses et parties sautées. Une visite partielle ne publie jamais une mesure de loudness du morceau entier. L’historique local s’appuie sur l’audio soumis et le temps actif, pas sur un accusé de lecture physique du périphérique.

## Répétition

`off` joue chaque occurrence une fois puis s’arrête. `one` recommence l’occurrence actuelle à sa fin naturelle. `all` entame un nouveau cycle complet après la dernière occurrence. Suivant contourne la répétition du morceau ; après la dernière occurrence, il arrête avec off ou one et commence le cycle suivant avec all. Arrêt termine toujours la session. Un fichier audio vide ne peut pas boucler indéfiniment.

Avec mélange et répétition complète, chaque cycle utilise une nouvelle graine dérivée de la précédente. Précédent peut revenir au cycle antérieur conservé ; avancer à nouveau réutilise le cycle suivant déjà préparé. File, modes et position ne sont pas persistés à la fermeture de la commande. Le worker d’historique séparé conserve au plus 64 événements en attente. Un stockage durablement lent freine la lecture au lieu de faire croître la mémoire indéfiniment. Cela peut retarder l’audio et les commandes jusqu’à la progression du stockage, particulièrement avec des morceaux répétés extrêmement courts.

## Les deux modes aléatoires

`random` est le mélange uniforme classique : une permutation de Fisher–Yates avec graine donne la même chance à chaque occurrence. Aucune restriction de genre ou d’artiste. Les doublons d’un M3U restent des occurrences distinctes, toutes jouées une fois par cycle.

`smart` privilégie de petits pas entre styles, puis varie les artistes et les albums parmi ces choix proches. Par exemple, une sélection contenant les enregistrements appropriés peut passer par classique → Third Stream → jazz → soul/funk → hip-hop → rap. Cela illustre des points de rencontre musicaux, pas une chronologie historique : la musique a plusieurs influences et l’année de sortie ne détermine pas son style. [Le NEC présente la rencontre classique/jazz du Third Stream](https://necmusic.edu/on-campus/library/archives-and-special-collections/archival-collections/gunther-schuller/) ; [la Library of Congress décrit plusieurs influences du hip-hop](https://www.loc.gov/collections/songs-of-america/articles-and-essays/musical-styles/popular-songs-of-the-day/hip-hop-rap/).

Le calcul utilise les genres et crédits du catalogue : aucune analyse audio, aucun BPM, service IA ou accès réseau. Les genres de chaque piste sont prioritaires ; l’union des genres d’une compilation ne rend pas tous ses morceaux similaires. Scanner d’abord et conserver des tags de piste utiles. Un fichier absent d’un catalogue existant reste dans la sélection avec des métadonnées inconnues. Sans genre utilisable, smart revient à un mélange uniforme et signale l’absence d’informations.

Ces modes pilotent `aede play`. Les clients Subsonic comme Submariner gèrent leurs propres commandes de répétition, déplacement et mélange ; leur bouton aléatoire habituel ne demande pas l’ordre intelligent d’Aède. PCM natif v1 transporte toujours un morceau par connexion, sans file ni commande de déplacement côté serveur.

Toutes les occurrences sont conservées. Si un morceau intermédiaire manque, a déjà été joué ou est épuisé, smart joue quand même les styles restants et signale les ruptures prévues, puis nomme la rupture au début de la transition concernée. Il signale aussi les transitions qu’il ne peut pas évaluer faute de genres. Cette heuristique est bornée : une rupture signalée ne prouve pas l’absence d’un meilleur trajet global. Elle n’invente pas de morceaux intermédiaires et ne réduit pas la sélection à un seul genre.

## Expert — mélange intelligent version 1

### Métadonnées et graphe de genres

Chaque profil conserve au plus huit genres normalisés, huit artistes principaux/invités et quatre labels, ainsi que l’identité de la piste, celle de l’album et son année. Les liens de genres directement attachés à la piste sont prioritaires ; le repli sur l’album et ses cooccurrences excluent les compilations. La normalisation traite casse, accents et ponctuation, avec des alias prudents comme R&B/rhythm and blues, hiphop/hip-hop et classique/classical. Un genre composé peut rejoindre un genre de base sur des mots complets ; une simple sous-chaîne ne suffit pas.

Un socle d’affinités non orientées comprend 69 genres et 96 liens, de coût 90 à 280. Ces liens et coûts sont des choix de lecture, pas des similarités scientifiquement mesurées ni une généalogie. Le graphe ajoute les genres personnalisés fréquents de la sélection, jusqu’à 512 nœuds au total. Les métadonnées omises ou limitées sont signalées. Les noms de genres bruts ou normalisés de plus de 256 octets UTF-8 sont omis et comptés comme métadonnées limitées ; un tag direct rejeté n’est pas remplacé par l’union des genres de l’album. Les noms normalisés sont mis en cache et partagés, et les profils bornés de labels sont préparés une fois par album sélectionné.

Les cooccurrences locales complètent ce socle : genres conjoints d’une piste de coût de base 140, contextes d’albums 220 et d’artistes 240. Un même ensemble de genres répété dans un album ne compte qu’une fois. Les associations d’albums/artistes demandent au moins deux contextes et ne peuvent pas créer de raccourci entre styles établis dont la distance préalable dépasse 300. Pour deux genres vus ensemble `c` fois, de fréquences `fA` et `fB`, le score entier de Dice est `s = floor(1000 × 2c / (fA + fB))`. Le lien coûte `base + floor(180 × (1000 − s) / 1000)` ; le lien le plus court est retenu.

Les plus courts chemins sont calculés par Dijkstra, avec une distance plafonnée à 1000. Deux genres déconnectés ont une distance de 1000 ; un genre manquant donne un résultat inconnu. Pour des pistes multigenres, la distance vaut un quart de trois fois la distance de la paire la plus proche, augmenté de la moyenne des distances au genre le plus proche dans les deux directions. Un enregistrement hybride peut donc servir de passerelle sans qu’un seul tag commun équivaille à une similarité totale.

### Candidats et pondération

Chaque étape examine au plus 96 candidats : jusqu’à six pour chacun des douze groupes de genres actifs les plus proches, huit sans genres et seize issus de toute la sélection restante, selon des priorités déterminées par la graine. Si la piste actuelle n’a pas de genres, les candidats viennent de la sélection restante. Le candidat connu le plus proche définit une bande locale de sa distance plus 60, plafonnée au pas préféré maximal de 300. Sans pas préféré disponible, des candidats plus éloignés restent possibles et la rupture est enregistrée.

Le poids entier de base est `(1100 − distance)²`. Une distance inconnue vaut 600 uniquement pour cette pondération. Un label commun ajoute 10 % ; un écart d’année inférieur à cinquante ans ajoute jusqu’à 10 % ; une collaboration connue entre artistes ajoute 12,5 %. Ces ajustements ne rendent jamais éligible un candidat extérieur à la bande de genres choisie.

Les artistes des cinq dernières occurrences et les albums des trois dernières subissent des pénalités souples dépendant de leur ancienneté. Une identité de piste récemment jouée voit son poids divisé par seize, ce qui conserve les doublons volontaires de playlist tout en tendant à les espacer. Une anticipation d’une étape divise aussi le poids d’un candidat par seize lorsqu’aucun groupe de genres restant ne permet un pas suivant préféré. Tous les poids restent positifs : aucun artiste, album ou morceau n’est exclu.

### Reproductibilité et limites

Les deux modes utilisent SplitMix64 et un tirage entier par rejet ; Fisher–Yates produit les priorités initiales. Une même sélection, les mêmes métadonnées de catalogue, graine et version d’algorithme reproduisent l’ordre sur chaque plateforme. Chaque cycle de répétition complète incrémente la graine modulo 2⁶⁴ de `0x9e3779b97f4a7c15` ; smart tient compte de la dernière occurrence précédente pour choisir son ouverture. Retaguer, modifier la sélection, intervenir pendant la lecture ou changer de version d’algorithme peut modifier le résultat.

Le graphe est borné et le calcul des candidats ne construit pas de matrice de distances entre toutes les pistes. Le moteur prépare les métadonnées avant l’audio et les réutilise entre cycles. Les plus courts chemins peuvent traverser des genres sans enregistrement sélectionné : ces nœuds influencent la distance mais n’ajoutent jamais de morceaux. La mémoire des artistes/albums couvre le plan actuel ; une frontière de répétition ne fournit que la dernière occurrence du cycle précédent. Le moteur privilégie des voisinages cohérents sans imposer une dérive chronométrée ni trouver un trajet optimal entre tous les styles. Tags personnalisés ou incohérents, absence de morceaux intermédiaires et échantillonnage borné des candidats limitent ses déductions. Il ne calcule qu’un ordre ; il ne modifie ni fichiers, ni tags, ni métadonnées stockées du catalogue.

## Résultat et erreurs

Le terminal affiche album et nom de fichier numéroté, 24 barres spectrales animées, étapes DSP actives, marges de normalisation/correction et mesures de sortie. Le compteur observe le PCM protégé soumis à la sortie avant dither/conversion de périphérique, pas le son mesuré au haut-parleur. Lire la source de normalisation et les interventions du garde pour comprendre un changement de niveau. La fin suit le mode de répétition choisi. Décodeur/sortie absent, disposition de canaux inconnue, playlist mal formée ou erreur de décodage/sortie produisent un diagnostic ; des écoutes terminées peuvent être déjà sauvées. Une sous-alimentation de périphérique est distincte d’un problème de tag. Le guide DSP précise les limites de mesure.

## Pour continuer

[track](track.md), [analyze](analyze.md), [history](history.md).

Guide détaillé existant : [design/playback.md](../../design/playback.md).
