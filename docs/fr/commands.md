<div id="commands-options-and-output" data-legacy-anchor></div>

# Commandes, options et sorties

<div id="where-each-option-applies" data-legacy-anchor></div>

## À quelles commandes s’applique chaque option

Les options se répartissent en trois groupes. Une option qu’une commande ne peut pas appliquer est **refusée**, jamais ignorée. Il en va de même pour un **argument** : auparavant, `aede artists ozzy` listait tous les artistes en laissant de côté le mot ajouté, ce qui ressemblait à une réponse. La commande indique désormais précisément quoi saisir, au lieu de vous laisser deviner.

La page principale `aede help` est un index : elle regroupe les options de bibliothèque, de récupération, d’images, de filtrage, de copie et d’import, sans transformer le terminal en manuel complet. Chaque commande possède sa page avec `aede help <command>` ou `aede <command> --help` ; les alias fonctionnent aussi. `aede help fetch` ajoute les détails propres aux métadonnées, paroles, images, options Fanart.tv et exclusions.

`aede help serve` explique l’installation, les limites de l’accès local et l’administration HTTP facultative. `aede serve` expose l’API locale du catalogue en HTTP/JSON/WebSocket. Sous Unix, les commandes CLI modifiant les fichiers de données sont automatiquement déléguées à ce serveur s’il fonctionne avec le même compte et le même dossier de données. La commande continue si sa CLI se déconnecte, y compris avec Ctrl-C ; `aede cancel <task-id>` arrête explicitement un scan ou une récupération déléguée. Les autres commandes déléguées ne disposent pas encore d’annulation explicite. Sans serveur, les commandes fonctionnent localement comme auparavant. Utilisez le même `--data <folder>` ou `AEDE_HOME` pour chaque commande, y compris `cancel`. Voir [Exploiter le serveur local](operating.md) pour le démarrage, les sauvegardes et la sécurité, ainsi que le [contrat de l’API](api.md) pour le comportement des clients.

`export` décrit toute la bibliothèque et n’accepte aucun argument. Sa sortie JSON ordinaire représente le **catalogue** dérivé ; `--csv` produit une ligne par album et `--tracks` une ligne par piste. `--graph` conserve au contraire le catalogue, les informations des sources, les décisions de validation, les données personnelles et une liste matérialisée de relations attribuées dans un même document.

Les **listes** — `albums`, `artists`, `genres`, `labels`, `years` — deviennent un tableau précis de ce qu’elles affichent, filtres compris. Vous pouvez ainsi regrouper plusieurs albums dans un fichier ciblé :

```sh
aede albums --csv --artist="Deicide" --output=deicide.csv
aede albums --csv --year=1990
aede albums --csv --compilations --output=compilations.csv
aede artists --csv --limit=100 --output=artists.csv
```

`album`, `artist`, `track` et `search` décrivent une **sélection**. `--csv` et `--m3u` s’appliquent tous deux : tableau de pistes ou playlist prête à être lue. Pour un artiste, la sélection contient les pistes sur lesquelles il est audible ; pour une recherche, elle contient les pistes trouvées, pas les artistes ou albums également trouvés.

`recording` et `work` parcourent le graphe musical canonique. Un enregistrement rassemble ses apparitions dans les albums locaux partageant son identité MusicBrainz d’enregistrement, et présente les relations d’œuvres attribuées ainsi que ses crédits d’enregistrement. Une œuvre rassemble les enregistrements qui interprètent la composition et affiche ses compositeurs, paroliers, auteurs et arrangeurs si MusicBrainz les fournit. `aede work` accepte aussi une œuvre obtenue avec `aede fetch --credits` : elle est clairement indiquée comme information externe, sans laisser croire que la récupération a ajouté un tag au fichier audio.

`aede fetch --credits` utilise les identifiants d’enregistrement et de sortie déjà présents dans les tags locaux. Elle conserve les interprètes et rôles de production sur l’enregistrement, les rôles créatifs sur l’œuvre et les rôles d’édition d’album sur la sortie exacte. Les noms tels que crédités, instruments ou qualificatifs, dates, ordre et identifiants de relations MusicBrainz sont préservés. Ces mêmes crédits attribués sont affichés par `track`, `album`, `artist`, `recording` et `work`. `track --json` les distingue des crédits lus localement dans les tags et identifie séparément ceux de l’édition. L’ancienne forme `--recordings` reste un alias.

`aede credit --add recording:<ID>|work:<ID>|release:<ID> --artist=<name>
--role=<role>` enregistre un crédit manuel attribué dans le périmètre indiqué ; `--artist-id=<MBID>` et `--instrument=<name>` sont facultatifs. Pour corriger une affirmation MusicBrainz, trouvez son identifiant de crédit stable avec `aede relations`, puis utilisez `aede credit --exclude=<ID>` et ajoutez au besoin un crédit manuel corrigé. `aede credit --undo=<ID>` rétablit l’affirmation exclue. L’exclusion agit sur la navigation du graphe et la recherche, tout en conservant l’information d’origine. Elle ne retire pas les crédits issus des tags locaux et ne réécrit aucun fichier. Les exclusions de crédits sont transportées par `aede rules --export` et `--import`.

`aede credits` vérifie la partie enregistrement/œuvre de cette passe sans contacter le réseau. La couverture des crédits d’édition n’est pas encore mesurée. La commande compte les enregistrements canoniques, pas chaque apparition sous forme de fichier, et liste les albums avec des compteurs distincts `credited`, `empty`, `waiting`, `untrusted` et `unidentified`. `aede credits "<album>"` détaille ces compteurs par enregistrement, avec un nom de fichier local représentatif. Elle sépare les crédits directs de l’enregistrement de ceux de l’œuvre, et propose une commande `fetch` limitée au dossier si des éléments sont en attente. `--json` fournit les mêmes états aux scripts. Une interrogation terminée sans crédits est signalée comme `empty`, pas comme une requête encore en attente. Utilisez `aede fetch --credits --full <folder>` uniquement si vous souhaitez réellement l’interroger de nouveau.

Pour les sorties classiques, la même interrogation d’enregistrement peut conserver une relation MusicBrainz explicite indiquant qu’une œuvre liée fait partie d’une autre. `aede work <parent ID>` liste alors les mouvements présents localement dans l’ordre de la source ; la page d’œuvre d’un mouvement renvoie vers son parent. La recherche `search` ordinaire liste les œuvres parentes uniquement connues des sources séparément des œuvres locales. `album` affiche les tags locaux d’œuvre, de regroupement et de mouvement à côté du parent attribué séparément. `track --json` exporte à la fois les tags et `parent_works`, avec la source et la confiance. `query work:<parent ID>` sélectionne les enregistrements locaux des parties identifiées. Une requête `work:<title>` peut également correspondre au tag WORK ou de regroupement propre à un fichier, et `movement:<title>` recherche son tag de mouvement, sans transformer ces titres en identité d’œuvre partagée. Ce comportement ne déduit pas une composition à partir d’un titre identique, ne transforme pas un *enregistrement* partiel en nouveau mouvement et ne modifie pas les tags audio. Au besoin, actualisez une ancienne réponse de relations avec `aede fetch --credits --full <album folder>`.

Les intervenants classiques correspondent à des rôles distincts : `orchestra`, `choir`, `ensemble`, `soloist`, `conductor` et `composer` peuvent chacun être recherchés par nom. Le champ `orchestra` reconnaît aussi le rôle MusicBrainz `performing orchestra`. `soloist` reconnaît son qualificatif explicite d’interprétation `solo`. `performing` inclut les participants à l’interprétation, y compris les chefs d’orchestre ; il ne signifie pas que chaque participant est littéralement audible.

Le graphe local se parcourt dans les deux directions. `track` indique l’enregistrement abstrait, ses œuvres et son groupe de sorties ; `recording` liste chacune de ses apparitions dans les albums locaux ; `work` liste tous les enregistrements ; `album` identifie le groupe de sorties et ses autres éditions locales ; `label` liste ses sorties. `artist` distingue discographie, participations invitées, présences sur des compilations, contributions d’écriture ou de production, collaborations et appartenances datées. La recherche `search` ordinaire trouve aussi les enregistrements, œuvres et groupes de sorties par leur titre ou identité.

Les contributeurs uniquement connus des sources possèdent aussi une page `artist`. Les crédits d’enregistrement, d’œuvre et d’édition approuvés les regroupent par identifiant MusicBrainz d’artiste ; leur fiche indique l’origine externe, les rôles, les albums locaux où ils apparaissent, les enregistrements et les œuvres. Le catalogue construit depuis les tags n’est pas modifié. `aede artist <MusicBrainz artist ID>` fournit un accès précis en cas d’homonymie ; `search` liste séparément les contributeurs externes et les artistes locaux, y compris avec `--json`. `track`, `album`, `recording` et `work` affichent des commandes copiables pour ouvrir chaque artiste crédité par une source approuvée, même si cette personne ne figure pas dans les tags locaux. Un enregistrement local sans MBID s’ouvre avec `aede recording 'local:<file path>'`, pour éviter l’ambiguïté entre enregistrements de même titre. Sur un artiste issu des sources, `--role` filtre ses enregistrements crédités ; `--members` et `--with` exigent des artistes locaux et sont explicitement refusés sur une fiche uniquement externe.

Si un artiste local existant porte le même nom qu’un contributeur uniquement externe, le nom ouvre toujours la fiche locale. Celle-ci présente un accès distinct, fondé sur l’identifiant, vers la fiche de la source, sans affirmer qu’il s’agit de la même personne.

Ce même graphe est disponible dans `query` : `recording`, `work` et `releasegroup` correspondent aux identités canoniques ; `instrument` aux attributs de crédits ; `guest`, `compilationartist`, `contributor` et `with` distinguent le mode de participation d’un artiste. Ces champs ramènent la relation aux pistes et peuvent donc être combinés avec les genres, années, labels et annotations :

```sh
aede query 'work:"War Pigs" instrument:guitar'
aede query 'guest:"Zakk Wylde" -compilationartist'
aede query 'label:"Epic" contributor:"Rick Rubin"'
```

Ces pages servent à naviguer, pas seulement à produire des rapports dans le terminal. Leur section `Continue` fournit des commandes copiables vers les objets voisins ; `search` inclut une commande `Open` pour chaque résultat nommé. Les identifiants MusicBrainz stables sont utilisés pour les enregistrements, œuvres, groupes de sorties et éditions précises lorsqu’ils existent. `release-group <title|MBID>` est la page intermédiaire entre l’identité d’un album et toutes ses éditions locales ; `album` accepte un MBID de sortie afin que deux pressages homonymes ne conduisent pas à une page ambiguë.

`label` applique la même règle d’identité. Il indique si l’identifiant MusicBrainz vient d’un tag local, a été confirmé par interrogation d’identifiant, reste une proposition de recherche par nom ou contredit le tag local. Propositions et contradictions ne sont jamais appliquées silencieusement. `aede review` les liste avec des identifiants stables, tandis que `aede review --interactive` rassemble les informations locales et externes dans une fiche. `--accept=<ID>` autorise cette affirmation précise pour la navigation et les requêtes ; `--reject=<ID>` la conserve seulement comme information ; `--undo=<ID>` annule l’une ou l’autre décision sans toucher aux fichiers. `aede doctor` signale également les propositions en attente, identités contradictoires, crédits récupérés incomplets et désaccords entre sources approuvées.

`relations` est l’inventaire sous-jacent aux pages d’entités. Il attribue un identifiant stable à chaque relation orientée locale ou externe, sans masquer l’origine de l’affirmation ni sa confiance. Un nom recherche les deux extrémités et le type de relation ; `--source` et `--tag` restreignent la liste :

```sh
aede relations "Andrew Watt"
aede relations --source=musicbrainz
aede relation <ID>
aede relation <ID> --text="Verify against booklet" --tag=dubious,liner-notes
aede relations --tag=dubious
```

`relation --remove` supprime toute l’annotation personnelle, tandis que `--tag=dubious --remove` retire seulement cette étiquette. La relation et ses extrémités ne sont jamais modifiées. Une annotation dont la relation disparaît est conservée et signalée par `doctor` ; son identifiant reste utilisable avec `relation --remove`.

`rules` distingue les choix humains reproductibles d’une sauvegarde complète. Sans option, la commande les liste ; elle les exporte dans un document versionné et fusionne ce document lors de l’import :

```sh
aede rules
aede rules --export --output=rules.json
aede rules --import=rules.json
```

Le document contient les décisions de validation des sources, les enregistrements de sources manuelles, les règles de classement des artistes, les sorties manquantes mises de côté, les requêtes enregistrées et les annotations de relations. Il exclut volontairement l’historique d’écoute et les textes récupérés en ligne : ce sont des données, pas des règles de correction.

```sh
aede album "To Hell With God" --csv --output=album.csv
aede artist "Deicide" --csv --separator=tab | sort -t$'\t' -k9,9n     # sorted by size
```

Utiliser `sort -t,` avec un CSV à virgules est un piège, ici comme ailleurs. Un titre contenant une virgule — réédition, album live ou nom comme « Compilation, Vol. 2 » — est correctement entouré de guillemets selon RFC 4180 (`"Once Upon the Cross, Reissue"`). Mais les outils ordinaires comme `sort` et `cut` ne comprennent pas la protection CSV : ils coupent à chaque virgule, même entre guillemets. Les colonnes se décalent et tous les champs suivants se retrouvent une position plus à droite. Cela fausse des tris précis comme `-k9,9n`, qui demande à `sort` d’isoler la neuvième colonne, ici la taille du fichier, puis de la trier numériquement plutôt qu’alphabétiquement. `--separator=tab` évite ce problème : aucun tag n’est séparé par des tabulations, donc un titre contenant des virgules ne nécessite plus de guillemets. Les données et le tri numérique `-k9,9n` restent alignés.

`aede album` accepte **un seul** titre : ses mots sont réunis, vous pouvez donc le saisir sans guillemets. Si vous en fournissez plusieurs, la commande vous rappelle laquelle permet de lister plusieurs albums.

La même logique s’applique aux options dont la valeur est un **nom** : `--artist`, `--album`, `--with`, `--genre` et `--label` prennent les mots suivants jusqu’à la prochaine option. Des options comme `--limit`, `--year` ou `--output` prennent exactement un mot, car un nombre ou un chemin constitue une valeur distincte.

```sh
aede artist Ozzy --with Zakk Wylde        # no quotes needed anywhere
aede artist Ozzy --with "Zakk Wylde"      # the same thing
aede track So What --artist Miles Davis --limit 1
```

Placez toujours l’argument positionnel avant l’option : `aede track --artist Miles Davis So What` donne toute la fin de la ligne à `--artist`. La commande signale alors qu’aucun titre n’a été fourni, plutôt que de prétendre répondre à une autre question.

`--output <file>`, ou `-o`, écrit dans un fichier plutôt que dans le terminal, mais seulement avec `--csv`, `--json` ou `--m3u` pour une sélection ou une liste, ou avec `export`, `sources --export`, `notes --export` et `rules --export`. Ce sont les seules commandes produisant du texte destiné à un fichier. Les autres sont des pages conçues pour l’écran ; `--output` y est explicitement refusé, jamais ignoré :

```
$ aede artist Ozzy --with Zakk Wylde -o test.txt
Error: --output writes what --csv, --json or --m3u produce; this page has none of those to give it.
Drop --output to see it on screen, or add one of the three to write it out.
```

`stats` est une autre page de consultation : elle présente l’état de la bibliothèque, sans fichier d’export associé :

```
$ aede stats

Library

  Tracks                        20
  Albums                         6
    of which compilations        1
  Artists                        8
  Total duration              38 s
  Size on disk              1.3 MB

Quality

                       Count      Size
  ───────────────────  ─────  ────────  ────────────────────
  Lossless (CD)           11  399.3 kB  ████████████████████
  Hi-res                   4  664.3 kB  ███████·············
  Lossy (>= 256 kbps)      3  248.3 kB  █████···············
```

<div id="getting-the-data-out" data-legacy-anchor></div>

## Exporter les données

Trois formats correspondent à trois façons d’interroger votre collection.

**JSON** (`aede export`) est l’export structurel fidèle du catalogue. Il constitue la matière première pour reconstruire le catalogue dérivé ou alimenter un autre programme.

**Graph JSON** (`aede export --graph`) conserve les trois voix dans un document : ce que disent les fichiers, ce qu’affirment les sources externes et ce que l’utilisateur a décidé ou annoté. Son tableau matérialisé `relations` facilite le parcours du graphe. Les sections de catalogue, de sources et d’utilisateur non aplaties préservent chaque détail et champ de provenance.

**CSV** (`aede export --csv`) est conçu pour un tableur et ses tris. Il produit **une ligne par album** : artiste, titre, année, nombres de pistes et de disques, durée, taille, formats, fréquences d’échantillonnage, résolutions, label, numéro de catalogue, genres, intégrité et dossier. Cette vue d’ensemble permet de trier par taille pour repérer ce qu’il faut extraire de nouveau, ou de filtrer sur `lossless` pour voir ce qu’il reste à améliorer. `--tracks` passe à une ligne par piste lorsque vous avez besoin du détail.

Les valeurs sont **brutes et explicites** : `duration_ms` et `size_bytes`, pas `4:20` et `31.2 MB`. Ces colonnes restent utilisables pour des calculs et des sommes.

Utilisez `--separator=;` pour Excel dans un environnement français ou allemand, et `--separator=tab` pour un TSV.

**M3U** (`--m3u`) transforme les données en playlist : les pistes affichées deviennent une liste de lecture.

```sh
aede album "To Hell With God" --m3u --output=deicide.m3u8
aede search coltrane --m3u --output=coltrane.m3u8
mpv --playlist=deicide.m3u8
```

Les chemins sont absolus : la playlist fonctionne quel que soit son emplacement d’enregistrement. `#EXTINF` transporte la durée et le titre, ce qui permet aux lecteurs de les afficher sans ouvrir chaque fichier. Sans `--output`, la playlist est envoyée sur la sortie standard et peut être transmise directement à un lecteur, par exemple `mpv --playlist=<(aede artist "Ozzy Osbourne" --m3u)`.

<div id="naming-a-folder" data-legacy-anchor></div>

## Indiquer un dossier

`check`, `spectrum`, `playlist`, `extract`, `fingerprint` et `analyze` acceptent tous des dossiers. Ces six commandes opèrent directement depuis le **catalogue**, sans réinventorier le disque. Un dossier inconnu du catalogue est immédiatement refusé, afin d’éviter une commande sans effet :

```
$ aede extract ~/Desktop/new-rips
Error: no file in the catalog is under "/Users/kcell/Desktop/new-rips".
It is on disk, so this catalog was scanned 3 days ago and has not seen it — a folder added since is not in it yet.
Add it: aede scan "/Users/kcell/Desktop/new-rips"
```

La date fournit le contexte nécessaire : vous pouvez comparer « scanné il y a trois jours » aux extractions réalisées depuis mardi.

<div id="paging-through-a-result" data-legacy-anchor></div>

## Parcourir les pages d’un résultat

Chaque liste affiche **50 lignes** par défaut et précise votre position :

```
  1–50 of 312 albums — --offset=50 for the next page, --all for every row
```

Trois options définissent une même fenêtre de résultats :

```sh
aede albums                        # the first 50
aede albums --limit 50 --offset 50 # the next 50
aede albums --all                  # every row, however many
aede albums --all --csv -o all.csv # and into a file
```

**Seule la situation de la page courante est indiquée.** Sur la dernière page, aucune suivante n’est proposée :

```
  301–312 of 312 albums — these are the last; drop --offset to start from the first
```

`--offset` donne un repère prévisible : l’ordre des listes est déterministe, donc la deuxième page continue la première. `--all` signifie simplement « tout », sans recourir à des conventions moins claires comme `--limit=0`, valeur refusée.

Si toute la requête tient sur une page, aucune indication supplémentaire n’est affichée. L’absence de message signifie que le résultat est complet. Si la fenêtre demandée dépasse la fin, un message l’indique explicitement, au lieu d’un écran vide qui pourrait faire croire à la perte de la bibliothèque.

`-o` est simplement la forme courte de `--output`.
