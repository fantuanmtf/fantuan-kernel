/* fantuan adaptation shim: x86_64 machine parameters (not upstream NetBSD).
 * The real amd64 header pulls <machine/cpu.h> under _KERNEL; this shim keeps
 * only the MI-visible constants so imported sources can compile without the
 * NetBSD CPU layer.
 */
#ifndef FANTUAN_MACHINE_PARAM_H
#define FANTUAN_MACHINE_PARAM_H

#define STACK_ALIGNBYTES (16 - 1)
#define ALIGNBYTES32 (sizeof(int) - 1)
#define PGSHIFT 12
#define NBPG (1 << PGSHIFT)
#define PGOFSET (NBPG - 1)
#define MAXIOMEM 0xffffffffffff
#define MAXPHYSMEM 0x100000000000ULL
#define KERNBASE 0xffffffff80000000
#define KERNTEXTOFF 0xffffffff80200000
#define SSIZE 1
#define SINCR 1
#define UPAGES 5
#define USPACE (UPAGES * NBPG)
#ifndef MSGBUFSIZE
#define MSGBUFSIZE (16 * NBPG)
#endif
#define MSIZE 512
#ifndef MCLSHIFT
#define MCLSHIFT 11
#endif
#define MCLBYTES (1 << MCLSHIFT)
#define NKMEMPAGES_MIN_DEFAULT ((8 * 1024 * 1024) >> PGSHIFT)
#define NKMEMPAGES_MAX_UNLIMITED 1

#define x86_round_page(x) ((((unsigned long)(x)) + PGOFSET) & ~PGOFSET)
#define x86_trunc_page(x) ((unsigned long)(x) & ~PGOFSET)
#define x86_btop(x) ((unsigned long)(x) >> PGSHIFT)
#define x86_ptob(x) ((unsigned long)(x) << PGSHIFT)
#define btop(x) x86_btop(x)
#define ptob(x) x86_ptob(x)

#include <machine/wchar_limits.h>
#include <machine/limits.h>

#endif
