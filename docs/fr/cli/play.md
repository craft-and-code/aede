# play — Écouter la musique

play accepte un fichier, un dossier parcouru récursivement dans l’ordre, un M3U/M3U8, une collection ou un nom catalogué. collection:NOM désigne explicitement une collection. Les chemins relatifs d’une playlist partent de son dossier, pas du terminal. Un M3U/M3U8 est limité à 16 Mio et doit être un fichier local ordinaire. Les chemins audio doivent être en UTF-8 valide pour éviter toute collision de clés dans l’historique et le cache de volume. Fichiers/dossiers se lisent sans scan ; noms et collections demandent un catalogue.

Dans les terminaux macOS/Linux et la console native Windows, les touches ci-dessous fonctionnent sans Entrée. Avec l’entrée standard redirigée, les commandes interactives sont désactivées et la sélection avance automatiquement avec les modes de répétition et de mélange demandés. Les écoutes sont enregistrées dans les données personnelles. La saisie console Windows est implémentée ; sa validation sur une console et un périphérique audio Windows réels reste à effectuer.

La lecture sans effets est le défaut : normalisation désactivée, graves et aigus neutres. Aède privilégie la fréquence source, mais peut effectuer les adaptations indispensables à la sortie ; ce mode pratique ne garantit pas le bit-perfect. Choisir `--playback=dsp` active la normalisation album automatique pour un album catalogué, ou track pour les autres sélections, vers -18 LUFS. Un réglage explicite de normalisation ou de tonalité non neutre choisit également DSP lorsqu’aucun mode n’est précisé. Les mesures actuelles FlacCompagnon, tags ou valeurs en cache peuvent fournir le gain ; une mesure manquante se calcule sans changer le niveau en cours de piste. Les hausses graves/aigus réservent une marge.

La sortie native ordinaire utilise CPAL lorsqu’il est disponible, sinon ffplay ; Opus/AAC/ALAC peuvent nécessiter ffmpeg. Les archives Linux statiques exigent ffplay et n’incluent pas la sortie ALSA directe stricte. Le niveau et le garde de sortie ordinaire signalent les dépassements d’échantillon ; aucun limiteur dynamique ni plafond true peak garanti. La lecture stricte contourne les protections qui modifieraient le signal. La continuité matérielle et l’identité de la sortie numérique restent à mesurer.

AEDE_AUDIO_BACKEND choisit la sortie locale : absent, essayer native puis ffplay ; native exige la sortie native sans repli ; ffplay choisit explicitement ce programme. Toute autre valeur est refusée. Exemple macOS/Linux : `AEDE_AUDIO_BACKEND=ffplay aede play "/path/to/track.flac"`. Avec PowerShell, définir `$env:AEDE_AUDIO_BACKEND = "ffplay"` avant play. Cela choisit la sortie, pas le décodeur ni la qualité DSP. Un `--output-device` explicite doit être respecté et désactive le repli. La lecture stricte exige toujours une route native admissible nommée et refuse ffplay imposé.

## Syntaxe et arguments

```text
aede play <file|folder|m3u|collection|artist|album|track> [--playback without-effects|bit-perfect|dsp] [--output-device ID] [--seek TIME] [--repeat off|one|all] [--shuffle off|random|smart] [--seed U64] [--lyrics] [--normalize off|track|album] [--bass DB] [--treble DB]
aede play --list-devices
```

Une sélection ; entourer de guillemets noms/chemins avec espaces. Préfixer une collection par collection:.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--playback without-effects\|bit-perfect\|dsp` | Choisir le mode de lecture local. Sans effets par défaut, sauf si des effets non neutres demandent explicitement DSP. Le mode bit-perfect strict exige un `--output-device` admissible. |
| `--output-device ID` | Choisir l’identifiant natif exact, préfixé par son backend, affiché par `--list-devices`. Un appareil absent ou incompatible produit une erreur ; aucun remplacement par défaut ni repli ffplay. |
| `--list-devices` | Afficher les noms et identifiants des sorties natives locales sans jouer d’audio. Utiliser seul, sans sélection ni options de lecture. |
| `--seek TIME` | Commencer la première occurrence lue à une position en secondes, `mm:ss` ou `hh:mm:ss`, avec jusqu’à trois décimales. Zéro par défaut. |
| `--repeat off\|one\|all` | Arrêter en fin de sélection, répéter le morceau ou répéter toute la sélection. off par défaut. |
| `--shuffle off\|random\|smart` | Garder l’ordre de sélection, mélanger uniformément ou privilégier des transitions progressives entre genres. off par défaut. Le mode smart demande un catalogue. |
| `--seed U64` | Reproduire un ordre mélangé avec une graine entière non signée sur 64 bits. Demande random ou smart. Sinon, une graine est générée et affichée. |
| `--lyrics` | Remplacer le spectre par les paroles locales synchronisées ou un aperçu de quatre lignes sans horodatage. Demande une sortie Terminal ; ne télécharge rien. |
| `--normalize off\|track\|album` | Choisir la normalisation. Off hors DSP ; en DSP, album par défaut pour un album catalogué, track sinon. Track/album explicite demande DSP si le mode est omis ; incompatible avec without-effects/bit-perfect explicite. |
| `--bass DB` | Réglage large des graves, de -12 à +12 dB. 0 est neutre ; les hausses réservent une marge. |
| `--treble DB` | Réglage large des aigus, de -12 à +12 dB. 0 est neutre ; les hausses réservent une marge. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

<div id="playback-policies-and-strict-output" data-legacy-anchor></div>

## Modes de lecture et sortie stricte

| Mode | Signal et incompatibilités |
| --- | --- |
| `without-effects` | Aucune correction de sonie ou de tonalité choisie. Privilégier la fréquence source ; conversion prise en charge, réduction des canaux et protection ordinaire peuvent rester nécessaires et sont signalées. |
| `bit-perfect` | Garder les échantillons entiers originaux, leur fréquence et l’ordre mono/stéréo. Aucun gain, EQ, rééchantillonnage, réduction de canaux, limiteur, écrêtage ni dither ; un élargissement exact du conteneur est permis. Toute source, route ou configuration incompatible est refusée. |
| `dsp` | Appliquer normalisation et tonalité choisies, avec adaptations de fréquence/canaux, marge et protection de sortie. |

Les options neutres (`--normalize=off --bass=0 --treble=0`) sont acceptées dans tous les modes. Sans `--playback`, `--normalize=track`, `--normalize=album` ou une valeur graves/aigus non nulle choisit DSP ; imposer le mode sans effets ou strict avec ces effets actifs produit une erreur. DSP explicite conserve la normalisation automatique existante, sauf remplacement par `--normalize=off`. Mode et appareil sont choisis pour cette commande ; aucune configuration persistante par appareil n’est ajoutée. PCM natif v1 et diffusion des originaux pilotée par le client gardent leurs contrats séparés.

Pour un FLAC à 96 kHz sur une sortie limitée à 48 kHz, le mode sans effets peut convertir et le signaler ; le mode strict refuse. Si représentations et route sont compatibles, sans effets peut conserver les échantillons, mais seul le mode strict impose le refus plutôt que l’adaptation.

La sortie entière native sans effets évite le dither pour les valeurs exactement représentables. Une conversion réduisant nécessairement la précision utilise encore le quantificateur existant et signale son nombre d’interventions ; voir [la sortie entière](../dsp/dither.md). La lecture stricte refuse ce compromis.

Le premier backend strict concerne Linux avec glibc et une sortie matérielle ALSA directe. Utiliser `aede play --list-devices`, puis copier un identifiant matériel tel que `alsa:hw:CARD=0,DEV=0` dans `--output-device`. La source doit être un FLAC natif ou un WAV PCM petit-boutiste, entier sur 16/24 bits, mono/stéréo. La route ouverte doit établir fréquence originale, précision effective suffisante, association des canaux et neutralité des contrôles pris en charge ; routes à plugins/mélangeur et faits inconnus sont refusés. macOS, Windows et binaires musl statiques refusent actuellement le mode strict, faute de backend direct admissible. Compiler la sortie native sous Linux glibc demande les en-têtes ALSA de développement et pkg-config.

Le matériel ALSA sélectionné doit exposer un contrôle lisible de la carte des canaux de lecture, cohérent avec le PCM ouvert. Une carte des canaux absente est refusée même pour un appareil stéréo ; une association configurée dans ALSA ne suffit pas à établir l’ordre matériel.

L’implémentation ALSA directe possède des tests de simulation logicielle et de signatures d’API. Compilation native Linux, édition des liens et fonctionnement réel de ce backend restent non vérifiés ; une configuration Linux simulée sur un autre système ne les valide pas. La capture numérique sur l’appareil visé constitue une étape d’acceptation supplémentaire et distincte.

Les erreurs strictes, interruptions de flux et changements de route arrêtent la préservation, sans activer silencieusement une conversion ou une reprise. Le mode est implémenté et contrôlé par des tests logiciels ; cela ne certifie pas tous les DAC : les captures numériques pour chaque matériel, pilote et réglage restent à effectuer. Aède ne change pas le volume système pour rendre la route admissible. Choisir un volume d’écoute adapté sur le DAC ou l’amplificateur avant la lecture ; conserver les échantillons ou limiter les true peaks ne garantit pas la protection du matériel.

La pause stricte exige une pause matérielle sans abandon de trames du programme. Si l’appareil ne le permet pas, Pause met fin au flux strict avec une erreur, au lieu d’ajouter un fondu, de redémarrer ou de sauter de l’audio.

Un MD5 audio FLAC présent est vérifié pendant le décodage dans les trois modes, indépendamment de la sortie. Son verdict devient définitif uniquement à la fin complète de la source, avant une fin normale réussie. Une somme absente reste inconnue ; ce contrôle ne compare pas continuellement le signal au DAC. Voir [l’intégrité pendant la lecture](../integrity.md#flac-audio-md5-during-playback).

Pour valider une route, suivre le [protocole technique d’acceptation stricte](../../coding/bit-perfect-acceptance.md), en conservant séparément les preuves de source, plateforme native et capture numérique. Le [guide de sortie](../dsp/output.md#bit-perfect-acceptance-evidence) résume ce qu’établit chaque contrôle. Une comparaison synthétique ou un retour logiciel n’est pas un verdict sur un DAC matériel.

## Exemples

```sh
aede play "$HOME/Music/album/01.flac"
aede play --list-devices
aede play "$HOME/Music/album/01.flac" --playback bit-perfect --output-device alsa:hw:CARD=0,DEV=0
aede play "Kind of Blue" --playback dsp
aede play "Kind of Blue" --normalize album
aede play collection:Road --bass 2 --treble -1
aede play "$HOME/Music/album/album.m3u" --normalize off
aede play "Kind of Blue" --seek 02:15.500
aede play collection:Road --shuffle random --repeat all
aede play collection:Journey --shuffle smart --seed 42
aede play "Kind of Blue" --lyrics
```

## Commandes du Terminal

Garder le terminal actif ; ces touches commandent directement Aède, sans Entrée. Windows Terminal et la console classique utilisent les événements clavier natifs ; PowerShell ISE et les autres hôtes sans entrée console ne fournissent pas ces commandes. Les lettres sont reconnues en minuscules et majuscules. Les relâchements de touche et les événements souris/fenêtre sans rapport ne déclenchent aucune action de lecture.

| Touche | Action |
| --- | --- |
| Espace | Pause/reprise. |
| `n` ou Droite | Occurrence suivante, même avec la répétition du morceau. |
| `p` ou Gauche | Occurrence précédente, ou recommencer le morceau après trois secondes. |
| `[` / `]` | Reculer/avancer de dix secondes dans le morceau. |
| `r` | Faire défiler la répétition : off → one → all → off. |
| `z` | Faire défiler le mélange : off → random → smart → off. Sans catalogue, smart est sauté avec une explication. |
| `q`, `s` ou Ctrl-C | Arrêter, sauvegarder l’historique et revenir au shell. |

Pendant la lecture interactive, la saisie n’est pas affichée. Aède restaure le mode d’entrée initial après un arrêt normal ou une erreur. Ctrl-C suit le même arrêt ordonné avec sauvegarde d’historique que `q` ; sous Windows, il est lu comme une touche console plutôt que terminer immédiatement le processus. Une erreur de lecture de l’entrée ou de restauration du mode est signalée.

Sous Windows, les crochets saisis avec AltGr sur un clavier français sont reconnus. La sélection à la souris de la console classique est désactivée pendant la lecture pour éviter qu’elle bloque les entrées/sorties et interrompe l’audio ; le réglage initial est rétabli à la sortie.

Répétition et mélange se changent aussi pendant une pause. Un changement de mélange conserve l’occurrence actuelle et celles déjà passées ; seules les occurrences restantes sont réordonnées. Désactiver le mélange restaure leur ordre relatif initial. Cela ne coupe pas le morceau en cours. Précédent suit l’ordre réellement joué, y compris le cycle précédent de répétition complète lorsqu’il est encore conservé.

## Se déplacer dans un morceau

`--seek` s’applique à la première occurrence effectivement jouée, y compris la première d’un ordre mélangé. Les morceaux suivants et les répétitions commencent à zéro. Reculer avant le début ramène à zéro ; avancer au-delà de la fin termine l’occurrence et suit le mode de répétition actif. Le déplacement conserve la pause.

Le décodeur rouvre la source puis décode progressivement le début pour l’écarter, avec une mémoire de travail bornée. Cette partie sautée ne passe ni dans le DSP, ni dans la sortie, ni dans l’historique. La position est arrondie vers le bas à une trame source ; son affichage utilise les millisecondes. Le déplacement n’est pas instantané par index : son coût augmente avec la position visée, et une lecture lente du fichier ou du décodeur peut retarder une commande entre deux contrôles d’annulation. Le déplacement réinitialise l’audio en attente et l’état DSP.

Plusieurs déplacements pendant une même visite créent une seule écoute incomplète, excluant pauses et parties sautées. Les segments inférieurs à une milliseconde sont cumulés avant l’arrondi final. Une visite partielle ne publie jamais une mesure de loudness du morceau entier. L’historique local ordinaire estime l’audio soumis pendant le temps actif et peut devancer la consommation native du tampon de sortie en attente (jusqu’à 500 ms). L’historique strict compte les trames source consommées par la sortie native ; fin normale de la source et consommation de la sortie sont nécessaires à la complétion. Aucun ne prouve la lecture physique du DAC. Le transport natif distant compte séparément les trames confirmées par le client.

## Répétition

`off` joue chaque occurrence une fois puis s’arrête. `one` recommence l’occurrence actuelle à sa fin naturelle. `all` entame un nouveau cycle complet après la dernière occurrence. Suivant contourne la répétition du morceau ; après la dernière occurrence, il arrête avec off ou one et commence le cycle suivant avec all. Arrêt termine toujours la session. Un fichier audio vide ne peut pas boucler indéfiniment.

Avec mélange et répétition complète, chaque cycle utilise une nouvelle graine dérivée de la précédente. Précédent peut revenir au cycle antérieur conservé ; avancer à nouveau réutilise le cycle suivant déjà préparé. File, modes et position ne sont pas persistés à la fermeture de la commande. Le worker d’historique séparé conserve au plus 64 événements en attente. Un stockage durablement lent freine la lecture au lieu de faire croître la mémoire indéfiniment. Cela peut retarder l’audio et les commandes jusqu’à la progression du stockage, particulièrement avec des morceaux répétés extrêmement courts.

## Les deux modes aléatoires

`random` est le mélange uniforme classique : une permutation de Fisher–Yates avec graine donne la même chance à chaque occurrence. Aucune restriction de genre ou d’artiste. Les doublons d’un M3U restent des occurrences distinctes, toutes jouées une fois par cycle.

`smart` privilégie de petits pas entre styles, puis varie les artistes et les albums parmi ces choix proches. Par exemple, une sélection contenant les enregistrements appropriés peut passer par classique → Third Stream → jazz → soul/funk → hip-hop → rap. Cela illustre des points de rencontre musicaux, pas une chronologie historique : la musique a plusieurs influences et l’année de sortie ne détermine pas son style. [Le NEC présente la rencontre classique/jazz du Third Stream](https://necmusic.edu/on-campus/library/archives-and-special-collections/archival-collections/gunther-schuller/) ; [la Library of Congress décrit plusieurs influences du hip-hop](https://www.loc.gov/collections/songs-of-america/articles-and-essays/musical-styles/popular-songs-of-the-day/hip-hop-rap/).

Le calcul utilise les genres et crédits du catalogue : aucune analyse audio, aucun BPM, service IA ou accès réseau. Les genres de chaque piste sont prioritaires ; l’union des genres d’une compilation ne rend pas tous ses morceaux similaires. Scanner d’abord et conserver des tags de piste utiles. Un fichier absent d’un catalogue existant reste dans la sélection avec des métadonnées inconnues. Sans genre utilisable, smart revient à un mélange uniforme et signale l’absence d’informations.

Ces modes pilotent `aede play`. Les clients Subsonic comme Submariner gèrent leurs propres commandes de répétition, déplacement et mélange ; leur bouton aléatoire habituel ne demande pas l’ordre intelligent d’Aède. PCM natif v1 accepte aussi des files ordonnées finies pour enchaîner les pistes compatibles. Son [extension interactive facultative](../server/playback.md#files-interactives-déplacement-et-reprise-persistante) ajoute déplacement, édition de la suite et reprise privée sauvegardée, avec des commandes client distinctes ; répétition et aléatoire natifs restent des choix du client. Les commandes CLI ne pilotent pas ce transport distant.

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

## Position de lecture

La lecture dans le Terminal affiche le morceau courant, la position écoulée et la durée totale, avec une barre de progression et un pourcentage. La position inclut le déplacement initial ou effectué pendant la lecture, reste figée pendant une pause et repart au début à chaque nouveau morceau ou répétition. Elle reste visible avec `--lyrics` ; une sortie redirigée n’affiche pas de progression animée.

Avec la sortie native, la position suit les compteurs de trames consommées, attribuées à l’occurrence courante plutôt qu’au morceau décodé en avance. Avec ffplay, `~` signale une estimation fondée sur le temps de lecture actif ; la mise en tampon et les blocages de sortie peuvent réduire sa précision. Aucune de ces horloges ne mesure la latence physique du périphérique. La durée totale vient d’abord des métadonnées du fichier, puis est corrigée à partir des trames source décodées à la fin, sans prédécoder le morceau. Une durée indisponible apparaît sous la forme `--:--`, sans pourcentage. Une durée totale trop courte limite la barre, sans masquer la position écoulée observée.

## Spectre dans le Terminal

La lecture dans le Terminal affiche un spectre Rétro à douze bandes larges et segmentées, des basses fréquences à gauche aux hautes fréquences à droite. Il regroupe les 24 bandes d’analyse existantes pour l’affichage, sans changer l’analyse ni le son. Chaque colonne se remplit depuis le bas, avec des segments verts en bas, jaunes plus haut et rouges au sommet. Un repère de crête distinct retombe plus lentement après la baisse du niveau courant.

La FFT s’exécute hors du callback audio. Ses instantanés attendent la même horloge de trames consommées que la progression : les échantillons en tampon n’animent donc plus les barres en avance sur la sortie native. Les enchaînements compatibles conservent les fenêtres d’analyse partielles et les crêtes ; déplacements, sauts et réinitialisations de sortie les écartent. La latence du périphérique et du mixeur reste inconnue ; ffplay utilise l’estimation signalée par la progression. À 48 kHz, une fenêtre de 2048 trames représente environ 43 ms, avec au plus 256 trames supplémentaires (environ 5 ms) pour son horodatage, puis un rafraîchissement du Terminal toutes les 50 ms. Ces intervalles logiciels ne garantissent pas une latence de bout en bout mesurée. Au plus 256 instantanés attendent la consommation ; un excès exceptionnel écarte les plus anciens sans retarder l’audio. La largeur du Terminal est vérifiée une fois par seconde. Le rendu reste sur le thread producteur : un Terminal bloqué ou une connexion SSH lente peut retarder l’audio lorsque le tampon de sortie est épuisé.

La largeur des bandes suit celle du Terminal. Un Terminal étroit regroupe les bandes au lieu de faire déborder l’affichage. `--no-color` ou `NO_COLOR` conserve les blocs et repères de crête en monochrome. Une sortie redirigée n’affiche pas de spectre animé ; `--lyrics` le remplace par les passages de paroles.

Les couleurs indiquent une hauteur d’affichage, pas un écrêtage ni un seuil de niveau calibré. Pour examiner les dépassements, consulter les mesures de sortie et les interventions du garde. L’animation n’ajoute aucune égalisation ni autre traitement audio.

## Paroles pendant la lecture

`--lyrics` affiche le passage LRC actif lorsqu’il change, à la place du spectre. Les lignes partageant un horodatage sont regroupées dans leur ordre source, avec jusqu’à quatre lignes par passage. Un passage horodaté vide efface les mots actifs. Les paroles sans horodatage donnent un aperçu de quatre lignes une fois par occurrence ; des paroles absentes ou invalides sont signalées sans arrêter la musique. Les lignes longues sont coupées à la largeur du Terminal. Les passages défilent avec la lecture ; cet affichage compact n’est pas un écran de karaoké complet.

Un tag de paroles non vide est prioritaire sur le `.lrc` adjacent. La lecture accepte au plus 256 Kio de texte source et 1 Mio après expansion des horodatages. Les informations du catalogue doivent toujours correspondre au fichier audio ; les fichiers lus directement sont examinés à nouveau. Aucun appel à LRCLIB ou à un autre service n’est effectué. Utiliser [fetch --lyrics](fetch.md) séparément pour récupérer les paroles manquantes, ou [track --lyrics](track.md) pour consulter le texte complet. Une sortie redirigée refuse `play --lyrics` ; la lecture sans cette option reste disponible.

Avec la sortie native, les paroles suivent les compteurs de trames consommées, en ajoutant la position du déplacement initial. Elles restent figées pendant une pause et se recalculent après un redémarrage, un déplacement ou un changement de piste. Les enchaînements compatibles et les répétitions naturelles conservent l’horloge de sortie tout en créant une nouvelle occurrence de paroles. Cela évite d’afficher le morceau suivant simplement parce que son décodage a commencé. L’anticipation des paroles contient au plus 64 occurrences ; si des morceaux très courts atteignent cette limite avant leur émission par le rééchantillonneur, Aède termine le groupe de traitement et attend la consommation audio. Le flux de sortie reste ouvert, mais les queues des filtres et les arrondis de conversion redémarrent à cette frontière exceptionnelle. La latence du périphérique et de l’hôte n’est pas mesurée : aucun alignement avec le son physique au DAC n’est garanti. Avec ffplay, le temps de lecture actif fournit une estimation annoncée explicitement ; la mise en tampon et les blocages de sortie peuvent réduire sa précision.

L’[API native des paroles](../server/playback.md) fournit à Phémios ou à un autre client autorisé une ressource de texte horodaté distincte. Le client suit sa propre position de présentation audio, ses pauses et ses déplacements. Les paroles ne sont pas insérées dans les paquets PCM ; cette API n’active pas les méthodes distinctes de paroles Subsonic.

## Intégrité audio FLAC pendant la lecture

Pour les sources FLAC, Aède compare l’audio décodé au MD5 stocké dans STREAMINFO pendant la même passe de décodage progressif. La comparaison utilise les échantillons entiers d’origine, avant conversion en flottants, normalisation, réduction des canaux, rééchantillonnage ou correction de tonalité. Elle ne modifie ni le fichier ni son son, ne prédécode pas tout le morceau et ne relance pas l’analyse complète de FlacCompagnon. Un résultat d’analyse précédent ne dispense pas de contrôler la source actuellement ouverte.

La vérification n’est concluante que lorsque la source complète atteint sa fin. Arrêter ou passer au morceau suivant avant cette fin laisse le contrôle en attente. Un déplacement décode progressivement le début écarté : atteindre ensuite la fin peut donc vérifier toute la source ; l’audio sauté reste absent de l’historique et de la mesure de volume. Un MD5 entièrement nul signifie qu’aucune signature n’est disponible : la lecture reste permise sans verdict vérifié. Une discordance produit un diagnostic de décodage, arrête la file et empêche de considérer l’écoute comme terminée normalement ou de sauver une nouvelle mesure complète de volume. Les écoutes précédemment terminées restent valides ; la durée soumise ou confirmée du morceau en erreur correspond à une écoute incomplète. Aucun verdict d’intégrité n’est écrit dans le catalogue.

Ce contrôle vérifie la cohérence de l’audio décodé, pas la qualité du mastering ni son authenticité. Le serveur ne décode pas les transferts de fichiers FLAC vers les clients Subsonic ; cette vérification de lecture concerne donc la lecture locale et le PCM distant natif. Voir le [guide d’intégrité](../../integrity.md#flac-audio-md5-during-playback) pour son périmètre exact et les limites du codec.

## Résultat et erreurs

Le terminal affiche album et nom de fichier numéroté, spectre Rétro (ou passages de paroles avec `--lyrics`), étapes DSP actives, marges de normalisation/correction et mesures de sortie. Le compteur observe le PCM protégé soumis à la sortie avant dither/conversion de périphérique, pas le son mesuré au haut-parleur. Lire la source de normalisation et les interventions du garde pour comprendre un changement de niveau. La fin suit le mode de répétition choisi. Décodeur/sortie absent, disposition de canaux inconnue, playlist mal formée ou erreur de décodage/sortie produisent un diagnostic ; des écoutes terminées peuvent être déjà sauvées. Une sous-alimentation de périphérique est distincte d’un problème de tag. Le guide DSP précise les limites de mesure.

## Pour continuer

[track](track.md), [analyze](analyze.md), [history](history.md).

Guide détaillé existant : [design/playback.md](../../design/playback.md).

## Sortie locale et appareils réseau

La lecture ordinaire utilise CPAL en priorité sur macOS, Windows et Linux glibc lorsqu’un appareil compatible peut être ouvert. ffplay sert de repli automatique/explicite et de sortie pour le binaire musl statique. `AEDE_AUDIO_BACKEND=native aede play /chemin/vers/morceau.flac` exige la sortie native et signale le problème au lieu de changer de sortie. Le mode strict dispose de la route ALSA directe Linux décrite plus haut. Les compteurs natifs suivent la consommation logicielle de sortie ; l’historique strict s’y rattache, tandis que l’historique ordinaire estime encore l’audio soumis. Aucun de ces compteurs ne prouve l’audibilité physique au DAC.

Pour un appareil SlimProto, UPnP AV ou OpenHome, utiliser la [diffusion vers un appareil](../server/devices.md). Les premiers profils transmettent les originaux encodés que l’appareil décode ; sortie CPAL locale et DSP serveur restent des fonctions distinctes.
