# Lire une sélection sur un appareil réseau

```sh
aede cast /chemin/vers/album --protocol slimproto --bind 192.168.1.10 --device 192.168.1.20 --device-volume 20
aede cast /chemin/vers/album --protocol upnp --bind 192.168.1.10 --device http://192.168.1.20:49152/device.xml
aede cast /chemin/vers/album --protocol openhome --bind 192.168.1.10 --device http://192.168.1.20:49152/device.xml
```

`--protocol`, `--bind` et `--device` sont obligatoires. SlimProto attend l’IP du lecteur ; UPnP/OpenHome son URL de description, disponible avec [`devices`](devices.md). Le lecteur SlimProto se connecte à Aède, normalement au port TCP 3483 ; `--port N` le change. `--device-volume 0..100` règle explicitement son gain numérique linéaire. Par défaut, le gain du lecteur est conservé, et un nouveau processus Squeezelite peut être muet. Les autres protocoles refusent ces options. `--replace` autorise le remplacement d’une file OpenHome existante et est refusé pour les autres protocoles.

Fichier, dossier, M3U/M3U8, collection ou artiste/album/piste catalogué sélectionne 1 à 64 occurrences ordonnées, avec les doublons volontaires. Les originaux encodés sont transmis inchangés pour décodage sur l’appareil. Ce premier profil n’applique ni DSP serveur ni transcodage, n’accepte ni répétition/aléatoire/déplacement et n’invente pas d’historique d’écoute. Les erreurs de source ou de format annoncé sont explicites. Garder Aède actif ; Ctrl-C demande Stop et ferme la session média temporaire. Les entrées OpenHome peuvent rester sur l’appareil, avec des URL expirées après l’arrêt d’Aède.

Consulter le [guide des appareils](../server/devices.md) avant un essai : codecs/fréquences SlimProto, état arrêté et file existante, limites réseau/secrets et procédure expérimentale pour la communauté. Gapless, multiroom et contrôle complet des appareils ne sont pas encore établis.
