# playlist — Créer des playlists

playlist écrit un .m3u portable dans chaque dossier d’album sélectionné. Les chemins sont relatifs à son dossier ; déplacer l’album et sa playlist conserve les liens. Une édition multi-disques reçoit une playlist au dossier d’album commun, disques dans l’ordre.

Par défaut, les lignes #EXTINF portent titre et durée. --simple garde seulement les chemins. --artists écrit aussi une discographie chronologique si les albums partagent un véritable dossier d’artiste ; aucun dossier n’est inventé et une racine plate n’est pas encombrée. --dry-run montre précisément les fichiers prévus sans écrire.

La commande crée/actualise des playlists annexes, pas des copies audio. Les droits des dossiers comptent. Pour exporter une sélection ailleurs, utiliser query --m3u --output ou collection --m3u. Pour lire le résultat, fournir son chemin M3U/M3U8 à play.

## Syntaxe et arguments

```text
aede playlist [folder…]
```

Dossiers musicaux catalogués facultatifs.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--dry-run` | Afficher le travail prévu sans télécharger ni créer de résultats. Les outils et entrées peuvent être vérifiés. |
| `--simple` | Écrire les chemins de playlist sans les lignes de durée/titre #EXTINF. |
| `--artists` | Créer aussi une playlist par dossier d’artiste commun, lorsqu’il peut être identifié sans ambiguïté. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede playlist "$HOME/Music/Jazz" --dry-run
aede playlist "$HOME/Music/Jazz" --artists
```

## Résultat et erreurs

Un chemin contenant un retour chariot ou saut de ligne est refusé avant toute écriture : M3U ne peut pas le représenter sans introduire une autre entrée. Les exports de sélection avec --m3u appliquent la même règle.

Le bilan distingue playlists prévues, écrites, à jour et échouées. --dry-run liste au plus les 20 premiers chemins prévus et compte la suite sans écrire ; les playlists identiques gardent leur date. Les éditions partageant un dossier contribuent à une playlist commune. Un fichier modifié est publié depuis une sortie temporaire isolée après écriture complète. Aucun album catalogué sélectionné : message explicatif et réussite. Les erreurs de fichiers sont listées et comptées, et tout échec produit un code de sortie non nul. La playlist est du texte UTF-8 avec un nom .m3u ; aucun audio copié.

## Pour continuer

[play](play.md), [query](query.md), [collection](collection.md).

Guide détaillé existant : [playlists.md](../../playlists.md).
