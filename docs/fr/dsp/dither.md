# Sortie entière et dither TPDF

Le flux traité est du PCM flottant `f32`. Un périphérique peut l’accepter directement ou exiger des entiers avec un nombre fini de niveaux. La **quantification** arrondit vers ces niveaux. À très faible amplitude, l’erreur d’arrondi déterministe peut suivre le signal et créer une distorsion.

## Ce qu’effectue Aède

La sortie native préfère le flottant compatible : ce parcours contourne quantification entière et dither. Si la sortie choisie impose l’entier, le callback convertit le PCM protégé avec un **dither TPDF** : deux tirages uniformes indépendants forment un bruit de distribution triangulaire, à l’échelle du bit de poids faible, avant arrondi. Ce petit bruit décorrèle l’erreur ; il n’ajoute pas de résolution à la source et ne répare pas le mastering.

Les formats natifs entiers signés/non signés de 8, 16, 24 et 32 bits sont pris en charge. Après la protection d’échantillons, la conversion borne le résultat à la plage entière choisie. L’état du dither continue entre blocs et pistes naturelles partageant la sortie. Le silence d’une pénurie de données reste un silence numérique exact, sans bruit ajouté.

```sh
aede play /chemin/vers/morceau.flac
```

Aucun réglage CLI de bits/dither n’existe actuellement. Les diagnostics précisent le format négocié. `AEDE_AUDIO_BACKEND=native` exige une sortie native au lieu du secours, sans choisir une profondeur particulière.

## Mesures et affirmations

Les mesures de sortie observent le PCM protégé **avant** dither/conversion du périphérique. Elles excluent donc arrondi/bruit final, mixeur système et reconstruction DAC. Le secours ffplay reçoit du `f32le` ; sa conversion ultérieure échappe à Aède.

Le parcours entier natif n’est pas revendiqué **bit-perfect** : l’audio est décodé en `f32`, puis intentionnellement dithéré vers l’entier. Normalisation, tonalité, mélange ou rééchantillonnage changent aussi les valeurs. Une tonalité plate seule ne prouve pas un acheminement intact dans système/périphérique.

Le modèle interactif exagère faibles profondeurs/bruit pour rendre l’arrondi visible. Le vrai dither opère à l’échelle du dernier bit du format ; l’escalier du dessin ne prédit pas des défauts audibles de votre lecture.

Voir [Sortie](output.md) et [Marge](headroom.md) pour les étapes de protection précédentes.
