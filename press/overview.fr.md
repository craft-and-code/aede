# Aède — Dossier de présentation

Aède est un moteur de bibliothèque musicale écrit en Rust. Il transforme les dossiers audio en catalogue relié : albums, artistes, pistes, crédits et relations. Il fonctionne localement en ligne de commande et peut exposer une API authentifiée pour les clients de bureau.

## Fonctions

Scan incrémental ; catalogue déterministe et métadonnées attribuées ; enrichissement externe explicite ; diagnostics Doctor ; comptes et données privées ; serveur HTTP local ou HTTPS configuré ; lecture, exports M3U et copie vérifiée ; analyses FlacCompagnon ; API de présentation pour Phémios.

## Approche

Les fichiers audio et leurs tags ne sont pas modifiés. Les données personnelles sont séparées du catalogue et liées à un compte. L’enrichissement externe est une action explicite ; une information manquante n’est pas présentée comme un résultat négatif.

## Disponibilité

Projet open source, licence MPL-2.0. Les versions disponibles sont publiées dans le dépôt officiel.

[Aède](https://craft-and-code.github.io/aede/) · [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/)
