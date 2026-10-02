# Normalisation : piste ou album

La normalisation réduit les écarts de niveau entre enregistrements en appliquant un gain constant pendant la lecture. Le **gain** amplifie/atténue, en dB : positif augmente, négatif diminue. Il ne change pas le volume système et ne comprime pas les passages calmes/forts d’un morceau.

## Choisir le périmètre

```sh
aede play "Un Album" --normalize album
aede play /chemin/vers/playlist.m3u --normalize track
aede play /chemin/vers/morceau.flac --normalize off
```

Le mode album préserve les différences relatives entre passages/pistes ; le mode piste rapproche leurs niveaux moyens indépendants. Un nom d’album catalogué utilise album par défaut. Les autres sélections utilisent track, y compris un dossier qui contient un album. `--normalize album` demande explicitement ce périmètre sans le déduire du chemin. Off contourne le gain de normalisation, pas l’égalisation, la conversion du périphérique ni la protection.

La cible actuelle est **−18 LUFS**, une référence de volume perçu, pas un plafond de crête. Une piste mesurée à −23 LUFS demande +5 dB ; à −13 LUFS elle demande −5 dB. La [marge de niveau](headroom.md) peut réduire une hausse : atteindre précisément −18 LUFS n’est pas garanti. LUFS pondère perceptuellement le programme ; il ne prédit pas le niveau acoustique de votre pièce.

## Tags et références

ReplayGain enregistre gain de piste/album et parfois crêtes correspondantes, avec référence nominale −18 LUFS. Les gains R128 d’Opus utilisent −23 LUFS et des valeurs à virgule fixe ; Aède les adapte à sa cible −18. Le gain obligatoire de l’en-tête Opus est distinct, appliqué par le décodeur avant la normalisation R128. Sur Opus, R128 passe avant ReplayGain si les deux existent.

Le sélecteur de métadonnées cherche le périmètre demandé, puis l’autre seulement s’il manque. En mode piste, la politique de lecture peut privilégier une mesure récente avant ce repli ; voir [Mesures](measurements.md). Un tag retenu mal formé/contradictoire provoque une erreur, pas un choix silencieux d’autre valeur. Aucun tag original n’est réécrit.

## Sans information prête

La lecture commence sans décoder toute la sélection à l’avance. Elle réutilise les tags du périmètre demandé ou des mesures actuelles. Le mode piste peut exploiter FlacCompagnon et un cache frais ; le volume source manquant est mesuré pendant l’écoute pour une **écoute ultérieure**. Le gain reste fixe pendant la piste, sans saut soudain à mi-chemin.

Le mode album demande des tags album complets ou une mesure en cache du programme ordonné complet. Aède ne moyenne jamais les LUFS des pistes pour inventer ceux de l’album. Sans données album prêtes, la session garde un niveau inchangé et peut apprendre le programme entier ininterrompu. Saut, arrêt, erreur de décodage ou fichier modifié empêchent de sauvegarder une capture partielle comme album complet.

Un morceau calme peut donc rester calme à la première écoute ou ne jamais atteindre la cible faute de marge de crête. Consultez source/gain/réserve affichés avant de conclure à une panne. Pour écouter sans correction de tonalité, ajoutez aussi `--bass 0 --treble 0`.

L’exemple interactif modifie des signaux synthétiques pour illustrer gain constant et réserve de crêtes ; il ne pilote pas la lecture.
