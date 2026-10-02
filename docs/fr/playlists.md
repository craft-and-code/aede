<div id="playlists-living-in-the-folders" data-legacy-anchor></div>

# Playlists : près des fichiers audio

`aede playlist` parcourt la bibliothèque cataloguée et crée une playlist `.m3u` prête à lire dans les dossiers d’album contenant la musique.

```text
aede playlist                       # un .m3u dans chaque dossier d’album
aede playlist ~/Music/Ozzy          # seulement sous ce chemin
aede playlist --simple              # chemins seuls, sans métadonnées #EXTINF
aede playlist --artists             # aussi une playlist de discographie par artiste
aede playlist --dry-run             # prévoir les écritures sans créer de fichier
```

Sans dossier en argument, la commande couvre tous les dossiers d’album du catalogue. Elle produit quelques kilo-octets de texte par album sans décoder l’audio. Comme elle peut écrire dans beaucoup de dossiers, commencer par `--dry-run` permet d’examiner le périmètre.

<div id="naming-and-portability-the-archivists-rules" data-legacy-anchor></div>

## Noms et portabilité des playlists

Le fichier porte le nom de son **dossier**, par exemple `1959 Kind of Blue [FLAC].m3u`, plutôt que le titre d’album lu dans les tags.

Ce choix répond à deux contraintes :

- **Noms autorisés :** un titre comme _1/2: The Early Years_ comporte des caractères interdits selon le système (`/`, `:`). Le dossier a déjà un nom accepté à l’endroit où la playlist est écrite.
- **Unicité :** deux dossiers peuvent contenir des albums de même titre, par exemple une extraction CD originale et une nouvelle édition haute résolution. Le nom du dossier permet à chacun de garder sa propre playlist.

Les chemins des pistes sont **relatifs** au dossier de la playlist. Déplacer ce dossier avec sa musique vers un disque, une carte micro-SD ou un emplacement partagé conserve ainsi les liens, tant que le lecteur peut accéder à ces fichiers.

_(Remarque : `--m3u` appliqué à une sélection de `aede search` ou `aede artist` produit au contraire des chemins absolus. Cet export peut être placé ailleurs sur le même système sans perdre ses références. Les deux méthodes partagent le même moteur de rendu, notamment pour les lignes de durée et de titre `#EXTINF`.)_

<div id="box-sets-and-discographies" data-legacy-anchor></div>

## Coffrets et discographies

L’ordre des pistes suit les tags : numéro de disque, puis numéro de piste.

**Un coffret garde une playlist commune.** Si une édition occupe les sous-dossiers `Disc 1` et `Disc 2`, Aède place la playlist dans leur **dossier d’album parent** et y inclut les deux disques. Les pistes numérotées de 1 à 17 sur chaque disque restent ainsi dans leur ordre de lecture.

Avec `--artists`, Aède cherche aussi un dossier commun aux albums d’un artiste pour y écrire une playlist de discographie. Si les albums sont dispersés ou partagent seulement une racine suivie telle que `~/Music`, cette playlist d’artiste n’est pas écrite. Une bibliothèque organisée à plat n’est ainsi pas encombrée de centaines de playlists à sa racine.

<div id="the-second-run-silent-and-safe" data-legacy-anchor></div>

## Relancer sans réécrire inutilement

**Une seconde exécution sur des données inchangées n’écrit rien.**

Aède compare le **texte** de la playlist, pas seulement l’existence du `.m3u`. Son contenu est calculé depuis les pistes actuelles du catalogue. Ajouter une piste, la scanner puis relancer la commande permet donc de mettre à jour la playlist si le nouveau texte diffère.

Si le texte est identique, le fichier reste intact, date de modification comprise. Les outils de sauvegarde et de synchronisation ne le considèrent pas comme modifié sans raison.
