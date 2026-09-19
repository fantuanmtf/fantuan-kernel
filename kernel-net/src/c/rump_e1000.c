/* rump_e1000.c - QEMU 82540EM (e1000) probe, reset and ring bring-up (ours).
 * The driver is interrupt-free: the net task polls the RX ring, so every
 * hardware step here is bounded.  PCI config space and the cache-disabled
 * BAR0 window come through the kernel-net Env hooks
 * (fantuan_rump_pci_read/write, fantuan_rump_mmio_map); the descriptor
 * rings and the packet buffers are contiguous frame-allocator pages
 * (fantuan_rump_pages_alloc) with their physical addresses via
 * fantuan_rump_virt_to_phys.  The stack runs with if_csum_flags_* = 0, so
 * ip_output and the input paths compute TCP/UDP/IP checksums in software
 * and the hardware only moves bytes.  The MAC/EEPROM and the send/receive
 * paths live in rump_e1000_dma.c. */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include "rump_shim.h"
#include "rump_e1000.h"
#include "rump_e1000_hw.h"

struct e1000_softc e1000_sc;

void e1000_hw_read_mac(void);

static int
e1000_pci_probe(uint8_t *devp, uint64_t *barp)
{
	static const uint16_t ids[] = {
		0x100e, 0x100f, 0x1010, 0x1012, 0x1015, 0x1016, 0x1017,
		0x1026, 0x1027, 0x1028, 0x10d3, 0x153a, 0
	};
	uint8_t dev, fn;
	uint32_t id, lo;
	int i;

	for (dev = 0; dev < 32; dev++) {
		for (fn = 0; fn < 8; fn++) {
			id = fantuan_rump_pci_read(0, dev, fn, 0);
			if ((id & 0xffff) == 0xffff) {
				if (fn == 0)
					break;
				continue;
			}
			if ((id & 0xffff) == E1000_VENDOR) {
				for (i = 0; ids[i] != 0; i++)
					if ((id >> 16) == ids[i])
						goto found;
			}
			if (fn == 0 &&
			    (fantuan_rump_pci_read(0, dev, 0, 0x0c) &
			    0x00800000) == 0)
				break;
		}
	}
	return ENXIO;

found:
	lo = fantuan_rump_pci_read(0, dev, fn, 0x10);
	if (lo & 1)
		return ENXIO;		/* I/O BAR: not the MMIO e1000 */
	*barp = lo & 0xfffffff0u;
	if ((lo & 0x06) == 0x04)
		*barp |= (uint64_t)fantuan_rump_pci_read(0, dev, fn, 0x14)
		    << 32;
	/* Memory space + bus master (firmware already placed BAR0). */
	id = fantuan_rump_pci_read(0, dev, fn, 0x04);
	fantuan_rump_pci_write(0, dev, fn, 0x04, id | 0x0002 | 0x0004);
	*devp = dev;
	return 0;
}

int
e1000_hw_init(void)
{
	uint8_t dev;
	uint64_t bar;
	uint32_t ctrl;
	int error, i;

	if (e1000_sc.ok)
		return 0;
	error = e1000_pci_probe(&dev, &bar);
	if (error != 0)
		return error;

	e1000_sc.regs = fantuan_rump_mmio_map(bar, 0x20000);
	if (e1000_sc.regs == NULL)
		return ENOMEM;

	ctrl = e1000_rd(E1000_CTRL);
	e1000_wr(E1000_CTRL, ctrl | E1000_CTRL_RST);
	for (i = 0; i < 100000; i++)
		if ((e1000_rd(E1000_CTRL) & E1000_CTRL_RST) == 0)
			break;
	if (i >= 100000)
		return ETIMEDOUT;
	e1000_wr(E1000_IMC, 0xffffffffu);
	(void)e1000_rd(E1000_ICR);
	e1000_hw_read_mac();

	e1000_sc.rx_ring = (struct e1000_rx_desc *)
	    fantuan_rump_pages_alloc(1);
	e1000_sc.tx_ring = (struct e1000_tx_desc *)
	    fantuan_rump_pages_alloc(1);
	e1000_sc.rx_bufs = (uint8_t *)fantuan_rump_pages_alloc(16);
	e1000_sc.tx_bufs = (uint8_t *)fantuan_rump_pages_alloc(16);
	if (e1000_sc.rx_ring == NULL || e1000_sc.tx_ring == NULL ||
	    e1000_sc.rx_bufs == NULL || e1000_sc.tx_bufs == NULL)
		return ENOMEM;
	e1000_sc.rx_ring_phys = fantuan_rump_virt_to_phys((void *)
	    e1000_sc.rx_ring);
	e1000_sc.tx_ring_phys = fantuan_rump_virt_to_phys((void *)
	    e1000_sc.tx_ring);
	e1000_sc.rx_bufs_phys = fantuan_rump_virt_to_phys((void *)
	    e1000_sc.rx_bufs);
	e1000_sc.tx_bufs_phys = fantuan_rump_virt_to_phys((void *)
	    e1000_sc.tx_bufs);

	memset(e1000_sc.rx_ring, 0, E1000_RX_DESC * sizeof(*e1000_sc.rx_ring));
	memset(e1000_sc.tx_ring, 0, E1000_TX_DESC * sizeof(*e1000_sc.tx_ring));
	for (i = 0; i < E1000_RX_DESC; i++)
		e1000_sc.rx_ring[i].addr =
		    e1000_sc.rx_bufs_phys + (uint64_t)i * E1000_BUF_SIZE;

	e1000_wr(E1000_RDBAL, (uint32_t)e1000_sc.rx_ring_phys);
	e1000_wr(E1000_RDBAH, (uint32_t)(e1000_sc.rx_ring_phys >> 32));
	e1000_wr(E1000_RDLEN, E1000_RX_DESC * sizeof(struct e1000_rx_desc));
	e1000_wr(E1000_RDH, 0);
	e1000_wr(E1000_RDT, E1000_RX_DESC - 1);
	e1000_wr(E1000_RCTL, E1000_RCTL_EN | E1000_RCTL_BAM |
	    E1000_RCTL_SECRC);

	e1000_wr(E1000_TDBAL, (uint32_t)e1000_sc.tx_ring_phys);
	e1000_wr(E1000_TDBAH, (uint32_t)(e1000_sc.tx_ring_phys >> 32));
	e1000_wr(E1000_TDLEN, E1000_TX_DESC * sizeof(struct e1000_tx_desc));
	e1000_wr(E1000_TDH, 0);
	e1000_wr(E1000_TDT, 0);
	e1000_wr(E1000_TIPG, 0x0060200au);
	e1000_wr(E1000_TCTL, E1000_TCTL_EN | E1000_TCTL_PSP |
	    E1000_TCTL_RTLC | (0x0fu << 4) | (0x40u << 12));

	e1000_sc.rx_cur = 0;
	e1000_sc.tx_cur = e1000_sc.tx_clean = e1000_sc.tx_used = 0;
	e1000_sc.ok = 1;
	return 0;
}
