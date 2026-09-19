/* fantuan adaptation shim: uvm/uvm_page.h - compile-only page type (ours,
 * not upstream).  Only the loan-free path names it, and that path is never
 * taken because uvm_loan() fails. */
#ifndef FANTUAN_UVM_UVM_PAGE_H
#define FANTUAN_UVM_UVM_PAGE_H

#include <sys/types.h>

struct vm_page;
typedef struct vm_page *vm_page_t;

#define VM_PAGE_TO_PHYS(pg)	((paddr_t)0)

#endif
