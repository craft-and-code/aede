# Discover network audio devices

```sh
aede devices --bind 192.168.1.10
```

Supply this computer's exact LAN IPv4 interface. The command sends one bounded SSDP discovery for UPnP AV/OpenHome renderers and lists each friendly name, implemented protocol and device-description URL. It performs no catalog scan, account change or background discovery.

Use a returned description URL with [`cast`](cast.md). SlimProto players connect explicitly to Aède and therefore do not appear in this SSDP list. No result can mean multicast is blocked, the wrong interface was chosen, or the player has no supported advertised service. Direct device URLs remain available when discovery is blocked.

The first profile uses private/link-local/loopback IPv4, without DNS, redirects or cross-host/port control URLs. Discovered descriptions must match the response peer. See [Network audio devices](../server/devices.md) for security boundaries and community trials. Hardware compatibility is experimental.
