# fetch — Enrichir depuis des sources

fetch déclenche explicitement le réseau. Sans option de passe, il interroge MusicBrainz sur artistes et albums. Choisir des passes exécute les familles demandées ; les paroles ne sont jamais activées implicitement. Les chemins existants limitent aux dossiers, les autres mots limitent aux noms d’artistes/albums. Les deux se combinent et restreignent chaque passe indépendamment.

Les réponses et images locales existantes sont réutilisées ; --full redemande les réponses, sans écraser les images locales. --dry-run liste les demandes sans les envoyer. Un gros traitement peut demander confirmation ; --yes l’accepte. Les réponses terminées sont sauvegardées progressivement : une reprise les réutilise. Avec le serveur Unix local, le travail survit à la déconnexion de la CLI et s’arrête via cancel.

Les identifiants MusicBrainz connus permettent des crédits précis d’enregistrement/œuvre/édition ; propositions approximatives et conflits restent des preuves séparées pour review. Les passes secondaires de discographie, résumés et images suivent seulement une identité exacte ou explicitement acceptée ; les propositions rejetées sont ignorées. Accepter une correspondance conserve son score initial. --credits et --recordings sont équivalents ; les demandes de crédits d’enregistrement correspondent aux noms d’artistes, aux titres d’albums et aux titres de pistes. --identify utilise les empreintes et AEDE_ACOUSTID_KEY ; --fanart/--logos exigent AEDE_FANARTTV_KEY. Garder les clés dans l’environnement, jamais une page publique ni URL.

Images et paroles .lrc sont annexes, sans réécriture de tags. Les nouvelles annexes sont publiées sans remplacer les fichiers ; le système doit accepter les liens physiques, sinon la publication est refusée (par exemple FAT/exFAT). Les images d’artiste vont dans un dossier commun si disponible, sinon dans assets des données. Les images d’album restent à côté/dans artwork/. Avec fetch, --images et --size exigent --covers. --banners exige --logos ou --fanart ; chaque exclusion --no-* exige --fanart. --logos/--banners ne se combinent pas avec leur exclusion négative correspondante. Les résumés conservent source, licence et langue. Un fetch exécuté par le serveur utilise ses chemins et son environnement, pas ceux du client.

`--covers` utilise d’abord l’identifiant d’édition, puis celui du groupe de parutions déjà présent dans les tags ou une source fiable. Une pochette supprimée depuis sa récupération peut réutiliser son adresse enregistrée. Un `--size` explicite ou `--full` redemande l’index : la largeur d’une ancienne miniature ne prend pas le dessus sur la nouvelle demande. Un front nouvellement confirmé remplace une ancienne réponse négative même si son transfert échoue, ce qui permet une reprise ordinaire. Les pixels JPEG et PNG statiques sont entièrement décodés avant publication, et leurs conteneurs doivent se terminer correctement. Les conteneurs incomplets et les flux de pixels non décodables sont refusés, ainsi que les PNG animés. Les images sont limitées à 32 Mio, 8192 pixels par axe et 16 millions de pixels ; choisir un `--size` plus petit si un original dépasse ces limites. Aucun outil externe n’est nécessaire. JPEG n’a pas de somme de contrôle d’intégrité : ce contrôle vérifie la décodabilité, sans certifier que l’original est intact.

La passe `--covers --images` conserve sa fin sous `coverartarchive-artwork` seulement après la réussite de toutes les images annexes, y compris une réponse valide sans image. La seule présence d’`artwork/` ne prouve pas que la passe est terminée. Une reprise ignore chaque fichier image déjà enregistré. Après suppression d’annexes à récupérer de nouveau, utiliser `--full` ; les images existantes restent intactes. Les paroles déjà sur disque sont ignorées même avant un rescan, et la recherche exige au moins une seconde entière de durée connue.

Une discographie est conservée seulement comme réponse complète : pages malformées, entrées répétées, totaux incohérents et limite de dix pages conservent la réponse précédente. Une réponse complète réussie, y compris une discographie vide, reste en cache après redémarrage ; `--full` la redemande. Les anciennes listes non vides restent utilisables. Une ancienne liste vide sans preuve de fin est récupérée une fois pour distinguer une réponse d’un artiste jamais consulté. Des entrées enregistrées illisibles invalident toute la discographie pour permettre une nouvelle demande ; les autres informations d’artiste restent disponibles.

## Syntaxe et arguments

```text
aede fetch [name… | folder…] [options]
```

Zéro ou plusieurs dossiers existants ou noms. Citer les noms composés. Un chemin doit exister pour être considéré comme dossier.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--yes` | Accepter la confirmation demandée par cette commande. Vérifier l’opération auparavant ; réserver aux exécutions automatisées maîtrisées. |
| `--full` | Redemander les réponses mémorisées ; ne jamais écraser images locales ou audio. |
| `--summaries` | Récupérer le texte introductif Wikipedia via Wikidata, avec langue, attribution et licence. |
| `--discography` | Récupérer les parutions MusicBrainz des artistes ; missing les compare au catalogue local. |
| `--covers` | Récupérer les pochettes manquantes depuis Cover Art Archive ; les images locales ne sont jamais remplacées. |
| `--identify` | Interroger AcoustID avec les empreintes mémorisées ; les calculer d’abord avec fingerprint. Nécessite AEDE_ACOUSTID_KEY. |
| `--credits` | Récupérer les crédits MusicBrainz d’enregistrement, composition et édition exacte, ainsi que les relations œuvre/partie à partir des identifiants connus. |
| `--recordings` | Ancienne écriture de --credits ; comportement identique. |
| `--portraits` | Récupérer un portrait d’artiste via Wikidata, puis Fanart.tv si nécessaire. |
| `--logos` | Récupérer les logos d’artistes et labels identifiés via Fanart.tv ; nécessite AEDE_FANARTTV_KEY. |
| `--fanart` | Récupérer les familles Fanart.tv : logos, portrait, bannière, fond, pochette et cdART. Nécessite sa clé API. |
| `--no-logo` | Exclure les logos d’artistes de --fanart. Nécessite --fanart. |
| `--no-label-logo` | Exclure les logos de labels de --fanart. Nécessite --fanart. |
| `--no-portrait` | Exclure les portraits de --fanart. Nécessite --fanart. |
| `--no-background` | Exclure les fonds de --fanart. Nécessite --fanart. |
| `--no-banner` | Exclure les bannières de --fanart. Nécessite --fanart. |
| `--no-album-cover` | Exclure les pochettes d’albums de --fanart. Nécessite --fanart. |
| `--no-cdart` | Exclure les images des disques de --fanart. Nécessite --fanart. |
| `--banners` | Récupérer aussi une bannière d’artiste avec --logos ou la passe complète --fanart. |
| `--labels` | Identifier les labels via MusicBrainz. Les propositions fondées sur le nom restent à examiner. |
| `--size VALUE` | Avec fetch --covers : 250, 500, 1200 (défaut) ou original. Avec spectrum : half (défaut) ou full. |
| `--lang CODE` | Langue préférée des résumés --summaries, par exemple fr. L’anglais reste le recours ; ce réglage seul ne crée pas de demande de résumé. |
| `--images` | Seulement avec --covers, télécharger aussi les images Cover Art Archive autres que la pochette dans artwork/. Refusé sans --covers. |
| `--dry-run` | Afficher le travail prévu sans télécharger ni créer de résultats. Les outils et entrées peuvent être vérifiés. |
| `--lyrics` | Avec track, afficher les paroles ; search, chercher leur texte ; fetch, télécharger les fichiers .lrc manquants depuis LRCLIB. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede fetch --dry-run "$HOME/Music/Jazz"
aede fetch --credits "$HOME/Music/Jazz"
aede fetch --summaries --lang fr "Miles Davis"
aede fetch --lyrics "$HOME/Music/Jazz"
aede fetch --covers --images "$HOME/Music/Jazz"
aede fetch --fanart --no-background --no-cdart
```

## Résultat et erreurs

Chaque passe nomme son périmètre et compte réponses conservées/téléchargées, refus et échecs. “Left alone” indique qu’aucune réponse univoque n’a été choisie, pas une modification de tags. Des échecs HTTP individuels sont comptés sans arrêter forcément les requêtes et passes suivantes : code 0 ne garantit pas leur réussite complète ; lire chaque bilan. Option inutilisable ou erreur de stockage arrête le traitement. HTTP 503 entraîne des attentes croissantes bornées ; un refus persistant arrête l’exécution. HTTP 429 l’arrête immédiatement sans cette séquence de reprises. Le travail terminé reste sauvegardé. --dry-run affiche les requêtes et “nothing was asked” sans créer d’annexes, de réponses ni le verrou d’écriture ; refuser la confirmation d’un grand lot ne lance rien. Rescanner les images/paroles téléchargées pour les rattacher à la navigation du catalogue. Examiner les propositions d’identité ambiguës avant de s’y fier.

## Pour continuer

[fingerprint](fingerprint.md), [credits](credits.md), [review](review.md), [missing](missing.md).

Guide détaillé existant : [sources.md](../../sources.md).
