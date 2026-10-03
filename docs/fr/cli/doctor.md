# doctor — Diagnostiquer la bibliothèque

doctor signale les problèmes sans correction automatique : tags manquants, doublons, albums incomplets, identités de sources non résolues, crédits incomplets, informations obsolètes ou orphelines et désaccords entre sources. Les niveaux sont error, warning et info.

Lire la raison et la commande proposée. Un doublon ou une suggestion de fusion ne supprime rien et ne fusionne personne. Un manque de crédits peut correspondre à un enregistrement non identifié ou à une recherche en attente ; credits les distingue avant de relancer le réseau.

--severity cible un niveau ; --all montre le rapport complet. --json conserve sa structure. doctor utilise le catalogue et les preuves mémorisées ; il n’interroge aucun fournisseur et ne vérifie pas tous les octets audio.

Une suggestion de doublon fondée sur les tags exige artiste, titre et durée positive lisible. Toutes les durées d’un groupe restent à trois secondes au plus de la plus courte ; des versions différentes ne sont pas réunies par une chaîne de durées intermédiaires. Cela reste une suggestion à examiner. Les pistes/disques manquants affichent les douze premières positions puis des points de suspension, même si un total erroné annonce des milliards de positions.

## Syntaxe et arguments

```text
aede doctor
```

Aucun argument positionnel.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--limit N` | Afficher au plus N détails ; entier strictement positif. Par défaut : 25, en affichage humain comme en JSON. Le bilan humain compte tous les problèmes filtrés. |
| `--offset N` | Ignorer N lignes avant le résultat ; N commence à 0. L’ordre reste déterministe. |
| `--all` | Afficher toutes les lignes. Incompatible avec --limit. Certaines commandes incluent aussi des catégories habituellement masquées, précisées ci-dessous. |
| `--json / -j` | Produire le tableau des diagnostics filtrés par niveau et paginés. `--all` demande tout ; une page au-delà de la fin donne `[]`. |
| `--severity error\|warning\|info` | Conserver seulement les diagnostics error, warning ou info. Cela filtre le rapport, sans rien corriger. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede doctor
aede doctor --severity warning --all
aede doctor --json
```

## Résultat et erreurs

L’affichage humain compte tous les problèmes filtrés par niveau, les regroupe par type puis présente la page de détails demandée. Les caractères de contrôle des métadonnées et chemins sont affichés littéralement ; le JSON conserve leurs valeurs originales. Une pagination invalide est refusée même si le filtre ne laisse aucun problème. Lire les raisons des diagnostics ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[credits](credits.md), [review](review.md), [check](check.md), [merge](merge.md).

Guide détaillé existant : [library.md](../../library.md).
