/* fantuan adaptation shim: x86_64 pmap (not upstream NetBSD).
 * Imported MI code only passes pmap_t around; the real page-table
 * implementation is not part of R1.
 */
#ifndef FANTUAN_MACHINE_PMAP_H
#define FANTUAN_MACHINE_PMAP_H

#include <sys/types.h>

struct pmap {
    int pm_dummy;
};

#endif
