/* fantuan adaptation shim: x86_64 machine context (not upstream NetBSD).
 * The imported MI slice only needs the type to exist; the real NetBSD
 * register layout is not reproduced until the signal path is ported.
 */
#ifndef FANTUAN_MACHINE_MCONTEXT_H
#define FANTUAN_MACHINE_MCONTEXT_H
typedef struct {
    unsigned long __md_gpr[16];
} mcontext_t;
#endif
