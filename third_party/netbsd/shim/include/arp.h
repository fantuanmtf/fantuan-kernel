/* fantuan adaptation shim: config-generated ARP count (not upstream).
 * config(1) emits this from the pseudo-devices that depend on arp; the
 * fantuan slice always configures the IPv4 ARP path (M11 R4), so NARP is 1
 * and the IPv4 attachment registers an ARP lltable. */
#ifndef FANTUAN_ARP_H
#define FANTUAN_ARP_H

#define ARP 1
#define NARP 1

#endif
