/* fantuan adaptation shim: x86_64 interrupt entry (not upstream NetBSD).
 * Only what the imported MI headers and the spike need; the real NetBSD
 * x86 interrupt source/APIC layer is deliberately absent.
 */
#ifndef FANTUAN_MACHINE_INTR_H
#define FANTUAN_MACHINE_INTR_H

#include <sys/types.h>
#include <machine/intrdefs.h>

typedef uint8_t ipl_t;
typedef struct {
    ipl_t _ipl;
} ipl_cookie_t;

int splraise(int);
void spllower(int);

static __inline ipl_cookie_t
makeiplcookie(ipl_t ipl)
{
    return (ipl_cookie_t){ ._ipl = ipl };
}

static __inline int
splraiseipl(ipl_cookie_t icookie)
{
    return splraise(icookie._ipl);
}

#define spl0() spllower(IPL_NONE)
#define splx(x) spllower(x)
#define SPL_ASSERT_BELOW(x) ((void)0)

void softintr(int);

#include <sys/spl.h>

#endif
