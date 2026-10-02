<div id="your-thoughts-your-data-annotating-in-aède" data-legacy-anchor></div>

# Vos commentaires et vos données : annoter dans Aède

Les notes personnelles complètent les fichiers audio. Vous pouvez écrire à propos d’une piste, d’un album, d’un artiste, d’un label ou d’un genre.

Une note par entité est conservée **telle que saisie**, lignes vides comprises. Aède ne reformate pas le texte enregistré, ne supprime pas discrètement les espaces et ne réorganise pas ses lignes.

```sh
aede note album "Kind of Blue" --text "the 1997 remaster is the one"
aede note album "Kind of Blue" --file notes/kind-of-blue.md
vim /tmp/note.md && aede note artist "Miles Davis" --file /tmp/note.md
somecommand | aede note artist "Miles Davis" --file - --append
aede note artist "Miles Davis"            # relire la note
aede note artist "Miles Davis" --remove
aede note album "Legion" --from album:"Once Upon the Cross"
```

Avec `--file`, rédiger dans l’éditeur de votre choix puis fournir le fichier. `-` permet de lire le texte transmis par un autre programme sur l’entrée standard. `--append` ajoute un nouveau passage sans remplacer les notes précédentes ; une ligne vide sépare les textes.

À l’affichage, les annotations personnelles sont distinctes des métadonnées techniques :

```text
Yours

  ★★★★★   ♥   vinyl

Notes

  # Kind of Blue

  The 1997 remaster is the one: the first three sides
  run fast on the original pressings.

  written 3 days ago
```

<div id="the-elegance-of-markdown-and-pure-ownership" data-legacy-anchor></div>

### Markdown et maîtrise du texte

**Markdown est le format de vos notes.** Aède conserve le texte fourni et le restitue sans réécriture. Une interface qui l’affiche peut mettre en forme titres, gras et emphase ; cette présentation reste séparée du stockage.

Deux principes en découlent :

1. **Sécurité :** le texte est une **entrée utilisateur non fiable**. Une interface web doit l’échapper correctement avant de l’insérer dans du HTML.
2. **Intégrité :** le stockage ne doit jamais reformater le texte à votre place. Un changement de présentation ne doit pas modifier la note conservée.

<div id="preserving-your-legacy" data-legacy-anchor></div>

## Conserver ses annotations

```sh
aede notes --export -o backup.json
aede notes --import backup.json
```

Un catalogue perdu peut être reconstruit en scannant la musique. Les annotations personnelles demandent une sauvegarde : les fichiers audio ne contiennent pas ces textes. L’export produit un document lisible, recherchable et modifiable manuellement.

**L’import fusionne les données.** Importer une sauvegarde partielle ne supprime pas les autres annotations déjà présentes.

Lorsque les deux jeux de données concernent la même entité :

- **Notes :** la note la plus récente est conservée. Les remplacements sont comptés et signalés dans le bilan.
- **Nombre d’écoutes :** le plus grand compteur est conservé ; importer une ancienne sauvegarde ne fait pas reculer ce nombre.

Importer une seconde fois la même sauvegarde ne change plus les données : les mêmes éléments ne créent pas de doublons.
