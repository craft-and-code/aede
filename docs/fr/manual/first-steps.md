# Votre première bibliothèque

Commencer par un petit dossier d’album bien tagué avant une grande archive. Remplacer chaque titre/chemin par un élément réellement présent. Les exemples utilisent `aede` dans le `PATH` ; depuis une archive, employer `./aede` ou `.\aede.exe`.

## 1. Construire le catalogue

```sh
aede scan "$HOME/Music"
aede stats
aede roots
```

`scan` lit récursivement et mémorise le dossier. `stats` vérifie les comptes ; `roots` montre ce qui sera lu. `aede scan` rafraîchit ensuite ces dossiers. Une racine absente/débranchée provoque un refus plutôt que vider silencieusement son catalogue. Un autre `scan DOSSIER` ajoute une racine, sans remplacer la première.

## 2. Trouver un album et naviguer

```sh
aede albums --limit 10
aede album "Kind of Blue"
aede track "So What" --artist "Miles Davis"
aede artist "Miles Davis"
aede search coltrane
```

Les pluriels listent ; les singuliers ouvrent. Les commandes `Open`/`Continue` copiables suivent les objets liés. Devant une édition ambiguë, employer son identifiant MusicBrainz affiché. Les tags décrivent les métadonnées locales ; les crédits externes restent attribués.

## 3. Écouter localement

```sh
aede play "Kind of Blue"
```

Terminal macOS/Linux : Espace pause/reprise, `n` suivant, `p` précédent/recommencer, `q` arrêt. Les touches Windows restent indisponibles. FFmpeg/ffplay peut être requis selon format/compilation. Un album utilise la normalisation album par défaut, gardant les différences de niveau entre pistes. [Référence de lecture](../cli/play.md) : gain, outils, limites.

## 4. Ajouter vos informations

```sh
aede love album "Kind of Blue"
aede rate album "Kind of Blue" --stars 5
aede tag album "Kind of Blue" evening,jazz
aede note album "Kind of Blue" --text "My preferred edition"
aede album "Kind of Blue"
```

Ces valeurs vont dans vos données, jamais les tags audio. Une note peut provenir d’un Markdown avec `--file notes.md`. Pour sélectionner selon une note d’album, utiliser `album.rating` ; `rating` seul cible les pistes. `loved` hérite volontairement d’un album/artiste favori.

## 5. Enregistrer une question

```sh
aede query "loved played:0"
aede collection Unheard --query "loved played:0"
aede collection Unheard
aede collection Unheard --m3u --output unheard.m3u8
```

La collection garde la question et la réévalue ensuite. Le M3U contient des chemins, pas l’audio copié. [copy](../cli/copy.md) crée une sélection distincte sur baladeur/carte.

## 6. Enrichir volontairement

```sh
aede fetch --credits "$HOME/Music"
aede fetch --summaries --lang fr "Miles Davis"
aede fetch --lyrics "$HOME/Music"
```

Ces commandes contactent les sources choisies. Les faits récupérés ne retaguent rien ; les paroles créent les `.lrc` manquants. Commencer par un dossier précis ou `--dry-run`. [fetch](../cli/fetch.md) explique clés, droits et confiance des preuves.

## 7. Préserver ce travail

```sh
aede backup aede-backup.aede
aede check
aede doctor
```

La sauvegarde Aède garde catalogue/conclusions/données personnelles/sources, pas la musique originale. Sauvegarder audio et annexes séparément. `check` inspecte les sommes compatibles ; `doctor` diagnostique tags/catalogue sans correction automatique. Continuer avec [usages quotidiens](workflows.md) et [référence des options](../cli/options.md).
