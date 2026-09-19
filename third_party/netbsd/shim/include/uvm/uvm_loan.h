/* fantuan adaptation shim: uvm/uvm_loan.h - compile-only loan prototypes
 * (ours, not upstream).  The UVM loan machinery is outside the slice, so the
 * adapter's uvm_loan() always fails and sosend() falls back to copying. */
#ifndef FANTUAN_UVM_UVM_LOAN_H
#define FANTUAN_UVM_UVM_LOAN_H

#include <sys/types.h>

struct vm_map;
struct vm_page;

#define UVM_LOAN_TOPAGE		0
#define UVM_LOAN_WIRED		1

int	uvm_loan(struct vm_map *, vaddr_t, vsize_t, struct vm_page **, int);
void	uvm_unloan(struct vm_page **, int, int);

#endif
