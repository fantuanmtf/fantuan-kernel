/* rump_ext.h - optional external TCP/TLS phase interface (ours, M11 R8). */
#ifndef FANTUAN_RUMP_EXT_H
#define FANTUAN_RUMP_EXT_H

void rump_ext_begin(void);
int rump_ext_poll(void);	/* 1 when the phase is finished */

#endif
