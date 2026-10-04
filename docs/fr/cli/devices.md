# Découvrir les appareils audio réseau

```sh
aede devices --bind 192.168.1.10
```

Indiquer l’interface IPv4 exacte de cette machine sur le réseau local. La commande effectue une recherche SSDP bornée de lecteurs UPnP AV/OpenHome, puis affiche nom, protocole pris en charge et URL de description. Elle ne scanne pas le catalogue, ne change pas les comptes et ne lance pas de découverte permanente.

Reprendre une URL affichée avec [`cast`](cast.md). Les lecteurs SlimProto se connectent explicitement à Aède et ne figurent donc pas dans cette liste SSDP. Une liste vide peut venir du multicast bloqué, d’une mauvaise interface ou d’un appareil sans service reconnu. Une URL explicite reste utilisable si la découverte est bloquée.

Ce premier profil utilise IPv4 privée/de lien local/de boucle locale, sans DNS, redirection ni contrôle vers un autre hôte/port. Les descriptions découvertes correspondent à leur émetteur. Le [guide des appareils réseau](../server/devices.md) détaille les limites et essais par la communauté. La compatibilité matérielle reste expérimentale.
