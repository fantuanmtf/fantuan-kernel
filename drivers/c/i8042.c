/* i8042.c — PS/2 keyboard driver (M8.5a, DESIGN.md §5/§12).
 *
 * Polling-free: kbd_init() configures the 8042 controller for IRQ1 with
 * scancode translation enabled, kbd_irq() is called from the Rust IRQ1
 * dispatch and translates set-1 make codes into ASCII, and kbd_getc() pops
 * them from a small ring buffer for the merged input source. Break codes
 * and extended (0xE0) sequences are ignored; shift works.
 *
 * Port I/O goes through the Rust core (k_inb/k_outb) — the C layer never
 * touches raw asm, matching the rust_core.h contract.
 */

#include <stdint.h>
#include <stddef.h>
#include "rust_core.h"

#define KBD_DATA   0x60u
#define KBD_STATUS 0x64u

#define ST_OUT_FULL  0x01u  /* output buffer has data for us */
#define ST_IN_FULL   0x02u  /* controller still busy with a command */

#define KBD_TAG      0x4B424431u  /* "KBD1" */
#define RING_SIZE    64u

static uint32_t g_tag;
static uint8_t g_ring[RING_SIZE];
static volatile uint32_t g_head;  /* producer (IRQ) */
static volatile uint32_t g_tail;  /* consumer (kbd_getc) */
static int g_shift;
static int g_extended;

/* --- controller helpers --------------------------------------------------- */

static int wait_in_clear(void)
{
    int i;
    for (i = 0; i < 1000; i++) {
        if ((k_inb(KBD_STATUS) & ST_IN_FULL) == 0) {
            return 0;
        }
        k_delay_ms(1);
    }
    return -1;
}

static int wait_out_full(void)
{
    int i;
    for (i = 0; i < 1000; i++) {
        if (k_inb(KBD_STATUS) & ST_OUT_FULL) {
            return 0;
        }
        k_delay_ms(1);
    }
    return -1;
}

static int ctrl_cmd(uint8_t cmd)
{
    if (wait_in_clear()) {
        return -1;
    }
    k_outb(KBD_STATUS, cmd);
    return 0;
}

static int ctrl_read(void)
{
    if (wait_out_full()) {
        return -1;
    }
    return (int)k_inb(KBD_DATA);
}

static void flush_output(void)
{
    int i;
    for (i = 0; i < 16 && (k_inb(KBD_STATUS) & ST_OUT_FULL); i++) {
        (void)k_inb(KBD_DATA);
    }
}

/* --- scancode set 1 (translation enabled in the config byte) -------------- */

static const char MAP[128] = {
    0,    27,   '1', '2', '3', '4', '5', '6', '7', '8', '9', '0',  '-',  '=',  '\b',
    '\t', 'q',  'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', '[',  ']',  '\n',
    0,    'a',  's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', ';', '\'', '`',
    0,    '\\', 'z', 'x', 'c', 'v', 'b', 'n', 'm', ',', '.', '/',  0,
    '*',  0,    ' ',
};

static const char MAP_SHIFT[128] = {
    0,    27,   '!', '@', '#', '$', '%', '^', '&', '*', '(', ')',  '_',  '+',  '\b',
    '\t', 'Q',  'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P', '{',  '}',  '\n',
    0,    'A',  'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ':', '"',  '~',
    0,    '|',  'Z', 'X', 'C', 'V', 'B', 'N', 'M', '<', '>', '?',  0,
    '*',  0,    ' ',
};

static void ring_push(uint8_t c)
{
    uint32_t next = (g_head + 1) % RING_SIZE;
    if (next == g_tail) {
        return;  /* full: drop the newest byte */
    }
    g_ring[g_head] = c;
    g_head = next;
}

/* --- exported interface --------------------------------------------------- */

int kbd_init(void)
{
    int config;

    flush_output();

    /* controller self-test (0xAA -> 0x55) */
    if (ctrl_cmd(0xAA) || (config = ctrl_read()) != 0x55) {
        k_log("kbd: controller self-test failed\n");
        return -1;
    }

    /* read the configuration byte: enable IRQ1 (bit 0) and translation
     * (bit 6), keep the keyboard clock enabled (clear bit 4). */
    if (ctrl_cmd(0x20) || (config = ctrl_read()) < 0) {
        k_log("kbd: cannot read config byte\n");
        return -1;
    }
    config = (config | 0x01 | 0x40) & ~0x10;
    if (ctrl_cmd(0x60) || wait_in_clear()) {
        k_log("kbd: cannot write config byte\n");
        return -1;
    }
    k_outb(KBD_DATA, (uint8_t)config);

    /* keyboard interface test (0xAB -> 0x00), then enable the keyboard */
    if (ctrl_cmd(0xAB) || ctrl_read() != 0x00) {
        k_log("kbd: interface test failed\n");
        return -1;
    }
    if (ctrl_cmd(0xAE)) {
        k_log("kbd: enable failed\n");
        return -1;
    }

    g_head = 0;
    g_tail = 0;
    g_shift = 0;
    g_extended = 0;
    g_tag = KBD_TAG;
    k_log("kbd: i8042 ready (IRQ1, scancode set 1 + translation)\n");
    return 0;
}

/* Called from the Rust IRQ1 dispatch: read one scancode, translate it, queue
 * the ASCII (if any) and return the key event as a packed int32 for the
 * kernel input ring (M13-3). -1 means "no key event" (no data / spurious /
 * extended prefix consumed). Packed layout: bits 0-7 scancode (make code),
 * bit 8 pressed flag, bits 16-23 scancode set, bits 24-31 translated ASCII. */
int32_t kbd_irq(void)
{
    uint8_t sc;
    char c = 0;

    if (g_tag != KBD_TAG) {
        return -1;
    }
    if ((k_inb(KBD_STATUS) & ST_OUT_FULL) == 0) {
        return -1;  /* shared with the mouse: spurious */
    }
    sc = k_inb(KBD_DATA);

    /* extended sequences (0xE0/0xE1 prefixes): skip the next byte */
    if (g_extended) {
        g_extended = 0;
        return -1;
    }
    if (sc == 0xE0 || sc == 0xE1) {
        g_extended = 1;
        return -1;
    }

    /* break codes have bit 7 set; track shift presses/releases */
    if (sc & 0x80) {
        uint8_t make = sc & 0x7F;
        if (make == 0x2A || make == 0x36) {
            g_shift = 0;
        }
        return (int32_t)make | (0 << 8) | (1 << 16);
    }
    if (sc == 0x2A || sc == 0x36) {
        g_shift = 1;
    }
    if (sc < 128) {
        c = g_shift ? MAP_SHIFT[sc] : MAP[sc];
    }
    if (c != 0) {
        ring_push((uint8_t)c);
    }
    return (int32_t)sc | (1 << 8) | (1 << 16) | ((int32_t)(uint8_t)c << 24);
}

/* Non-blocking pop; -1 when empty. */
int kbd_getc(void)
{
    uint8_t c;
    if (g_tag != KBD_TAG || g_tail == g_head) {
        return -1;
    }
    c = g_ring[g_tail];
    g_tail = (g_tail + 1) % RING_SIZE;
    return (int)c;
}
