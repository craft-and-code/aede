# DSP Premium : pistes futures

Cette page présente une proposition produit/recherche, **pas des commandes Premium disponibles ni un contrat de sortie validé**. Prix, droits, dates et périmètre final restent à décider. Les autres pages DSP décrivent les traitements présents, distincts de ces idées.

## Premières priorités envisagées

| Proposition | Possibilité | Travail préalable |
| --- | --- | --- |
| Égaliseur paramétrique | Plusieurs filtres cloche/plateau/passe-haut/passe-bas, avec fréquence, gain, Q et bypass. | Stabilité, marge sûre, presets et changements progressifs sans clic. |
| Profils par périphérique | Réglages casque/enceintes/sortie séparés, import/export et profil actif lisible. | Validation, format/version et changements sûrs. |
| Convolution d’impulsions personnelles | Appliquer un filtre FIR de réponse de casque/pièce. | Latence bornée, politique de fréquence, transitions et validation de réponse. Ce moteur seul n’est pas une correction automatique de pièce. |

Le **Q** décrit la largeur d’un filtre paramétrique. Une **réponse impulsionnelle** décrit un filtre dans le temps ; la convolution l’applique à la musique. Ces filtres demandent des entrées mesurées/attribuées, pas une affirmation que toute courbe téléchargée améliore le son.

## Autres recherches

L’égalisation par modèle de casque dépend des mesures, licences et de son placement. Le crossfeed mélangerait une part retardée/filtrée de chaque côté vers l’autre pour écouter au casque. Un limiteur de crête vraie afficherait anticipation, latence, plafond et réduction de gain ; il diffère de la protection actuelle d’échantillons.

La compression nocturne pourrait réduire le contraste fort/calme dans certaines situations. Un fondu optionnel mélangerait des titres indépendants avec courbes réglables, tout en préservant par défaut les albums continus. Balance/niveaux/délais/polarité et gestion des graves demandent des sorties adaptées. La correction assistée de pièce exige microphone calibré, plusieurs positions et expertise acoustique ; elle n’est pas promise pour la première sortie.

Égalisation dynamique/compensation de volume, largeur mid/side, vitesse/hauteur et rendu binaural/spatial ajoutent des besoins perceptuels, de calcul et de données. Ce sont des candidats de recherche. Aucune option CLI/API actuelle ne les active ; le transport serveur futur est un prérequis distinct, pas un effet DSP Premium.

## Valider les évolutions

Conserver un bypass explicite et les fichiers/tags intacts. Mesurer réponse, stabilité, repliement, crêtes, périmètres de normalisation, jonctions et indépendance de taille des blocs. Afficher étapes/latence/profil réellement actifs. Préférences d’écoute et fidélité demandent une validation perceptuelle, pas seulement des courbes attrayantes. Jonctions matérielles et budgets NAS restent à tester même pour la base présente.

La [proposition DSP](../../design/dsp-product-proposal.md) rassemble recherche, priorités envisagées et sources. Elle servira aux mises à jour ; ses pourcentages sont des estimations de planification, pas des qualités mesurées ni garanties de disponibilité.
