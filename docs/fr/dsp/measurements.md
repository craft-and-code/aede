# Mesures source et FlacCompagnon

Les **LUFS intégrés** mesurent le volume perceptuel d’un programme avec des seuils excluant les passages suffisamment faibles. La **crête d’échantillon** est la plus grande valeur PCM enregistrée. La **crête vraie** estime la forme reconstruite entre échantillons, qui peut dépasser cette valeur. Ces mesures répondent à des questions différentes ; aucune ne prouve une meilleure qualité sonore.

## Réutiliser l’analyse sans changer les fichiers

```sh
aede analyze /chemin/vers/album --json
aede scan /chemin/vers/musique
aede play /chemin/vers/morceau.flac --normalize track
```

`analyze` exécute l’analyse acoustique [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/) sur les albums catalogués ; `--json` enregistre aussi un rapport d’album. Scan importe les rapports reconnus et conserve leurs résultats attribués dans les conclusions. Ils peuvent apparaître dans le détail de piste et fournir LUFS/crête vraie au DSP s’ils correspondent toujours au fichier. Ce n’est pas une écriture de tags ReplayGain. Le [guide d’import](../../imported-analyses.md) décrit reconnaissance, dates, attribution et données conservées.

En lecture piste, un tag valide du périmètre demandé est prioritaire. Sans lui, Aède consulte l’analyse FlacCompagnon actuelle, puis le cache dérivé frais. L’autre périmètre de métadonnées peut servir de repli si aucune mesure appropriée n’est prête. Les tags retenus invalides sont signalés, pas masqués. Pour l’album, la donnée doit couvrir tout le programme ordonné : on ne calcule pas ses LUFS en moyennant ceux des pistes.

## Apprendre pendant l’écoute

Le volume manquant est mesuré sur le PCM original décodé **avant** mélange des canaux, conversion de fréquence, gain et tonalité. Aucun décodage intégral préalable ne retarde le démarrage. Le gain actuel reste fixe ; l’apprentissage sert à une écoute suivante. La publication s’effectue hors boucle de décodage, après décodage complet et vérification du fichier inchangé (chemin, taille, date de modification).

Une piste interrompue/sautée/en erreur n’est pas enregistrée comme mesure complète. La capture album exige toutes ses pistes dans l’ordre, sans interruption. Sans tags/cache album valides, le niveau actuel reste inchangé plutôt que s’adapter piste par piste. Un fichier modifié invalide la capture. Silence, programme insuffisant ou agencement source non pris en charge peuvent empêcher une mesure LUFS exploitable : l’inconnu reste explicite, sans inventer les positions des canaux.

La méthode actuelle du cache dérivé est version 4 ; les versions antérieures sont rejetées car certaines limites de décodage/crêtes finales pouvaient donner des valeurs incorrectes. Les analyses FlacCompagnon importées restent distinctes et ne sont pas effacées par cette invalidation. Un résultat importé périmé demeure une information, sans devenir silencieusement une décision de gain actuelle.

## Limites des mesures

Les LUFS intégrés fonctionnent à haute fréquence. Le compteur de crête vraie actuel ne suréchantillonne plus à **192 kHz et au-delà** : la crête vraie y reste volontairement inconnue. La crête d’échantillon n’est pas renommée crête vraie. Les mesures de sources finies résolvent la fin retardée d’interpolation dans un compteur séparé de crêtes ; aucun silence n’est ajouté à la lecture ni faux contenu aux seuils LUFS.

Les mesures source décrivent le programme original. Les [mesures de sortie](output.md) décrivent le PCM protégé transmis avant conversion du périphérique. Aucune ne mesure DAC/enceintes analogiques. La lecture FLAC vérifie un MD5 audio présent pendant le même décodage, avant ces mesures et le DSP, puis le finalise uniquement à la fin complète de la source. Une discordance empêche de sauver une nouvelle mesure complète de volume. Signature absente ou décodage interrompu ne constituent pas des verdicts vérifiés. Le [guide d’intégrité](../../integrity.md#flac-audio-md5-during-playback) et la [commande check](../cli/check.md) distinguent le contrôle de l’audio décodé de celui du conteneur ; une lecture réussie n’est pas un certificat complet du fichier.
