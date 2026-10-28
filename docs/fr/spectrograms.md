# Spectrogrammes PNG natifs

Aède partage le calcul et le rendu PNG Rust de FlacCompagnon via sa dépendance
flaccompagnon-core et le crate indépendant flaccompagnon-spectrogram inclus
dans la même version. Aucun dépôt voisin, processus Python, moteur graphique
externe ou police installée n’est nécessaire.

## Génération

```sh
aede spectrum
aede spectrum ~/Music/Album --dry-run
aede spectrum ~/Music/Album --size standard
aede spectrum ~/Music/Album --size full --full
```

La zone spectrale verte mesure 1200×627 par défaut ; le grand format conserve
1800×940. Le cadre ajoute la signature Aède, le titre, les propriétés audio
réellement décodées, les axes temps/fréquences et la légende des niveaux.
L’ancienne option --size half reste un alias de standard. Le PNG est sans perte.

L’échelle −120…0 dBFS/bin est fixe, sans gain automatique par morceau. Les canaux
sont transformés séparément puis leurs puissances sont combinées, pour conserver
le contenu en opposition de phase. Les métadonnées PNG décrivent les réglages.
Une coupure spectrale ne prouve pas, isolément, une origine avec perte.

## Dépendances et fichiers

Dans la version candidate intégrée, le PCM natif et le profil DSF/DFF DSD64/128
mono/stéréo ne demandent plus FFmpeg. La mesure DSD emploie le PCM large bande
352,8/705,6 kHz, séparément du profil d’écoute. WavPack, Musepack, Speex et l’APE
moderne utilisent aussi le lecteur commun dans ses profils admis. Les autres
configurations conservent le décodage de compatibilité optionnel. Une piste en
échec ne supprime pas les images terminées des autres pistes.

Les PNG restent dans spectrograms/ à côté des sources, ouvertes en lecture
seule. Une sortie temporaire privée est publiée uniquement après un succès.
Les images existantes sont préservées en cas d’erreur ; les liens et sorties
spéciales sont refusés avant le travail et avant la publication. Cette politique
ne constitue pas un sandbox contre le remplacement hostile de dossiers parents.

Sans chemin, la commande considère le catalogue. --threads règle les traitements
simultanés ; --dry-run permet d’en voir la portée. Une image récente est
conservée selon son empreinte de version, la taille de la source et sa date
complète, nanosecondes comprises. Les anciennes images sans empreinte sont
régénérées une fois. Un changement de taille seul conserve une image à jour ;
--full permet de la régénérer. Les versions de mesure/dessin invalident les
anciens résultats.

Le crate partagé accepte dimensions, fenêtre, recouvrement, plage et échelle de
fréquences, canaux, réduction des pixels, palettes, limites en dB, textes,
police, marges, axes, légende, grille et annotations mesurées. Aède choisit pour
l’instant les réglages verts communs ; sa CLI expose la taille.

Le serveur conserve ses routes PNG authentifiées. Phémios affiche les nouvelles
dimensions sans changement de protocole ; son moteur embarqué devra être
reconstruit après publication de la version correspondante d’Aède.

Le décodage progressif utilise deux passages bornés, sans charger un morceau
entier ni écrire d’audio converti. Une annulation coopérative, une source modifiée
ou une erreur native préserve l’ancienne image ; aucune récupération externe
d’un préfixe plus court n’est publiée après admission native. Les API partagées
attendent une publication coordonnée ; les versions publiées et les Apps déjà
installées ne changent pas lors de cette intégration de sources.
