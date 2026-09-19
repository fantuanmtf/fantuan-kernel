/* rump_e1000_reg.h - 8254x register/descriptor definitions (ours). */
#ifndef FANTUAN_RUMP_E1000_REG_H
#define FANTUAN_RUMP_E1000_REG_H

#define E1000_VENDOR		0x8086

#define E1000_CTRL		0x0000
#define E1000_STATUS		0x0008
#define E1000_EERD		0x0014
#define E1000_ICR		0x00c0
#define E1000_IMC		0x00d8
#define E1000_RCTL		0x0100
#define E1000_TCTL		0x0400
#define E1000_TIPG		0x0410
#define E1000_RDBAL		0x2800
#define E1000_RDBAH		0x2804
#define E1000_RDLEN		0x2808
#define E1000_RDH		0x2810
#define E1000_RDT		0x2818
#define E1000_TDBAL		0x3800
#define E1000_TDBAH		0x3804
#define E1000_TDLEN		0x3808
#define E1000_TDH		0x3810
#define E1000_TDT		0x3818
#define E1000_RAL		0x5400
#define E1000_RAH		0x5404

#define E1000_CTRL_RST		(1u << 26)
#define E1000_EERD_START	(1u << 0)
#define E1000_EERD_DONE		(1u << 1)
#define E1000_RCTL_EN		(1u << 1)
#define E1000_RCTL_BAM		(1u << 15)
#define E1000_RCTL_SECRC	(1u << 26)
#define E1000_TCTL_EN		(1u << 1)
#define E1000_TCTL_PSP		(1u << 3)
#define E1000_TCTL_RTLC		(1u << 24)
#define E1000_RXD_STAT_DD	0x01
#define E1000_TXD_STAT_DD	0x01
#define E1000_TXD_CMD_EOP	0x01
#define E1000_TXD_CMD_IFCS	0x02
#define E1000_TXD_CMD_RS	0x08
#define E1000_STATUS_LU		(1u << 1)

struct e1000_rx_desc {
	uint64_t addr;
	uint16_t length;
	uint16_t csum;
	uint8_t status;
	uint8_t errors;
	uint16_t special;
} __attribute__((packed));

struct e1000_tx_desc {
	uint64_t addr;
	uint16_t length;
	uint8_t cso;
	uint8_t cmd;
	uint8_t status;
	uint8_t css;
	uint16_t special;
} __attribute__((packed));

#endif
