# Cast a selection to a network player

```sh
aede cast /path/to/album --protocol slimproto --bind 192.168.1.10 --device 192.168.1.20 --device-volume 20
aede cast /path/to/album --protocol upnp --bind 192.168.1.10 --device http://192.168.1.20:49152/device.xml
aede cast /path/to/album --protocol openhome --bind 192.168.1.10 --device http://192.168.1.20:49152/device.xml
```

`--protocol`, `--bind` and `--device` are mandatory. SlimProto takes the expected player's IP; UPnP/OpenHome take its description URL, available through [`devices`](devices.md). SlimProto players must connect to Aède, normally at TCP port 3483; `--port N` changes it. `--device-volume 0..100` explicitly sets SlimProto linear digital gain; default preserves the device's gain, which can be muted on a fresh Squeezelite process. Other protocols reject these options. `--replace` authorizes replacing an existing OpenHome playlist and is refused for other protocols.

A file, folder, M3U/M3U8, collection or catalogued artist/album/track selects 1–64 ordered occurrences, including intentional duplicates. Original encoded bytes are transferred unchanged for device decoding. There is no server DSP, transcoding, repeat/shuffle/seek option, or inferred listening history in this first profile. Source or advertised format errors are explicit. Keep Aède running; Ctrl-C requests Stop and ends the temporary media session. Completed OpenHome entries can remain on the device, with URLs that expire when Aède exits.

Read the [device guide](../server/devices.md) before trying a new renderer: it covers native SlimProto codec/rate limits, stopped-device/playlist policies, LAN/capability boundaries and the experimental community test procedure. Gapless, multiroom and complete device-control support are not established.
