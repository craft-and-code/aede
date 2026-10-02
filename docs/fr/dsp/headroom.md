# Marge de niveau, saturation et protection finale

En PCM flottant, la pleine échelle nominale va de −1 à +1. La **marge de niveau** est l’espace restant avant de dépasser cette plage. Un signal trop amplifié peut **écrêter** en sortie bornée : les crêtes sont aplaties, avec distorsion. Aède rend sa politique explicite plutôt que supposer chaque gain demandé sans risque.

## Limiter d’abord le gain de normalisation

Aède utilise la crête ReplayGain déclarée, une crête vraie mesurée valide ou suppose prudemment la pleine échelle si aucune n’existe. Le gain est limité pour que cette crête ne dépasse pas 1.0. Avec une crête 0,5, environ +6 dB sont disponibles ; une demande +9 est donc plafonnée vers +6. Sans crête connue, supposée 1.0, une hausse positive est évitée.

C’est une décision **statique**, fondée sur l’information disponible. Un tag peut être erroné ; une crête d’échantillon sous-estimer celle entre échantillons. Ce plafond ne garantit pas la sécurité en crête vraie. La cible −18 LUFS peut rester hors d’atteinte sans erreur.

## Réserver ensuite la tonalité

Les hausses de graves/aigus réservent leur somme en atténuation préalable : +6/+3 dB réservent 9 dB ; des baisses seules réservent zéro. Cette marge est distincte du plafond de normalisation. Elle réduit le risque sans modifier dynamiquement le contraste calme/fort. Voir [Tonalité](tone.md).

## Protéger enfin les échantillons transmis

Après traitement, la protection finale borne durement les valeurs hors `[−1,+1]` et compte les échantillons modifiés. Les valeurs dans la plage restent intactes. Le DSP flottant intermédiaire garde sa marge ; ce plafond est explicite avant sortie native ou ffplay. Échantillons non finis, trames incomplètes et gains débordants sont refusés plutôt qu’envoyés comme audio valide.

Un compteur supérieur à zéro signale une intervention réelle ; ce n’est pas une réduction de gain en dB. La CLI affiche aussi la crête avant protection, pour rendre le dépassement visible. Essayez une tonalité plate et examinez la source/normalisation avant de demander davantage de hausse.

```sh
aede play "Un Album" --normalize off --bass 0 --treble 0
```

Cette comparaison retire gain/égalisation facultatifs en conservant protection et conversion nécessaire. Elle n’est pas un contournement dangereux et ne prouve pas un parcours bit-perfect.

## Protection contre limiteur

Un **limiteur de crête vraie** avec anticipation analyserait les crêtes à venir et diminuerait progressivement le gain sous un plafond, avec latence et indicateur de réduction. Aède ne possède pas ce traitement aujourd’hui. La protection dure d’échantillons ne garantit pas un plafond reconstruit ; la réduction dynamique est explicitement indisponible. Une crête vraie peut dépasser la pleine échelle même si aucun échantillon stocké ne la dépasse.

Les indicateurs observent le PCM protégé transmis avant dither/périphérique. La crête vraie est inconnue à 192 kHz et au-delà. [Mesures](measurements.md) et [Sortie](output.md) détaillent ces limites ; le [limiteur Premium futur](premium.md) reste une proposition.
