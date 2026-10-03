# sources — Comprendre les preuves externes

sources conserve une seconde couche de propositions attribuées à côté des tags. Le résumé compte les éléments rattachés au catalogue et ceux en attente. --list montre chaque source, confiance et rattachement. Une correspondance approximative ne devient jamais silencieusement vérité locale.

--export écrit le document de sources, y compris les décisions de revue et les exclusions de crédits lorsqu’aucune preuve récupérée ne reste ; --import fusionne un document du même format. --template crée des entrées vides munies de clés, éventuellement limitées à un nom en argument, pour une saisie manuelle précise. Choisir une seule opération : list, export, import, template ou forget. --output sert avec export/template. --source filtre list/forget ou nomme la source du modèle ; il est refusé avec le résumé, l’export ou l’import, qui concernent toutes les sources.

--forget retire les preuves et leurs décisions de revue/exclusion dans la portée choisie, y compris les décisions conservées sans preuve en cache. Les preuves et décisions supprimées sont comptées séparément ; aucun tag ni fichier musical ne change. Aucun nom positionnel sauf avec --template NOM : ouvrir album/artist/track pour consulter une entité. review accepte une proposition, credit corrige un crédit précis, backup sauvegarde tout.

Le résumé, la liste, l’export et le modèle lisent les données sans prendre de verrou d’écriture. Import et forget conservent ce verrou pendant les modifications.

## Syntaxe et arguments

```text
aede sources
```

Aucun argument positionnel sauf un nom facultatif avec --template.

## Options de cette commande

| Option | Effet |
| --- | --- |
| `--output FILE / -o FILE` | Écrire l’export dans FICHIER plutôt que dans le terminal. Une sélection exige --csv, --json ou --m3u ; les exports propres à une commande suivent leurs règles. |
| `--forget` | Retirer les données/décisions mémorisées dans cette portée. Aucun fichier audio n’est supprimé ni retagué. |
| `--list` | Lister les enregistrements ou décisions mémorisés au lieu du résumé/de l’opération habituelle. |
| `--source NAME` | Filtrer --list/--forget ou choisir la source de --template. Refusé avec le résumé, l’export ou l’import. |
| `--export` | Exporter le document mémorisé propre à cette commande. Ce n’est pas le même résultat qu’une liste affichée en CSV/JSON. |
| `--template` | Créer un document de sources manuelles vide pour les entités, éventuellement limité au nom fourni en argument. |
| `--import FILE` | Fusionner un document exporté depuis FICHIER ; consulter les règles de conflit précisées ci-dessous. |

La [référence des options](options.md) explique `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, valeurs et règles d’export/pagination. Elles ne rendent pas CSV/JSON ou pagination disponibles partout.

## Exemples

```sh
aede sources
aede sources --list --source musicbrainz
aede sources --export --output sources.json
aede sources --template "Miles Davis" --output manual-sources.json
aede sources --import manual-sources.json
```

## Résultat et erreurs

La sortie décrit l’opération ou les données choisies. Lire avertissements et bilan du travail conservé ; finir le processus ne garantit ni intégrité indépendante ni qualité sonore. Erreurs de syntaxe/options : généralement code 2 ; échecs de traitement : généralement code 1. Une interruption locale diffère d’une tâche serveur déléguée, comme expliqué dans [cancel](cancel.md).

## Pour continuer

[fetch](fetch.md), [review](review.md), [credit](credit.md), [backup](backup.md).

Guide détaillé existant : [sources.md](../../sources.md).
