/* rump_shim_mem.c - frame-backed kernel memory services (ours).
 * kmem/vmem/kern_malloc are a single linear (bump) arena: R2 only needs a
 * bounded, working allocator and the pool backend needs real page free, so
 * uvm_km_kmem_alloc maps directly onto the frame allocator instead. Freed
 * bump items are not reused; R3 replaces this with a real vmem/kmem.
 */
#include <sys/types.h>
#include <sys/errno.h>
#include <sys/param.h>
#include <sys/kmem.h>
#include <sys/malloc.h>
#include <sys/mbuf.h>
#include <sys/systm.h>
#include <sys/vmem.h>
#include <uvm/uvm_extern.h>
#include "rump_shim.h"

#define KMEM_ALIGN 16

struct vmem {
	int vm_dummy;
};

static struct vmem meta_arena_store;
static struct vmem va_arena_store;
vmem_t *kmem_meta_arena = &meta_arena_store;
vmem_t *kmem_va_arena = &va_arena_store;

psize_t physmem;
int nkmempages;

static char *kmem_bump;
static size_t kmem_left;

static void *
kmem_bump_alloc(size_t size)
{
	void *p;
	size_t pages;

	size = (size + KMEM_ALIGN - 1) & ~(size_t)(KMEM_ALIGN - 1);
	if (size < KMEM_ALIGN)
		size = KMEM_ALIGN;
	if (size > kmem_left) {
		pages = (size + PAGE_SIZE - 1) >> PGSHIFT;
		p = fantuan_rump_pages_alloc(pages);
		if (p == NULL)
			return NULL;
		kmem_bump = p;
		kmem_left = pages << PGSHIFT;
	}
	p = kmem_bump;
	kmem_bump += size;
	kmem_left -= size;
	return p;
}

void *
kmem_alloc(size_t size, km_flag_t flags)
{

	(void)flags;
	return kmem_bump_alloc(size);
}

void *
kmem_zalloc(size_t size, km_flag_t flags)
{
	void *p = kmem_alloc(size, flags);

	if (p != NULL)
		memset(p, 0, size);
	return p;
}

void
kmem_free(void *p, size_t size)
{

	(void)p;
	(void)size;
}

void *
kern_malloc(unsigned long size, int flags)
{

	(void)flags;
	return kmem_bump_alloc((size_t)size);
}

void
kern_free(void *p)
{

	(void)p;
}

int
vmem_alloc(vmem_t *vm, vmem_size_t size, vm_flag_t flags, vmem_addr_t *addr)
{
	void *p;

	(void)vm;
	(void)flags;
	p = kmem_bump_alloc((size_t)size);
	if (p == NULL)
		return ENOMEM;
	*addr = (vmem_addr_t)(uintptr_t)p;
	return 0;
}

void
vmem_free(vmem_t *vm, vmem_addr_t addr, vmem_size_t size)
{

	(void)vm;
	(void)addr;
	(void)size;
}

int
uvm_km_kmem_alloc(vmem_t *vm, vmem_size_t size, vm_flag_t flags,
    vmem_addr_t *addr)
{
	size_t pages = ((size_t)size + PAGE_SIZE - 1) >> PGSHIFT;
	void *p;

	(void)vm;
	(void)flags;
	p = fantuan_rump_pages_alloc(pages);
	if (p == NULL)
		return ENOMEM;
	*addr = (vmem_addr_t)(uintptr_t)p;
	return 0;
}

void
uvm_km_kmem_free(vmem_t *vm, vmem_addr_t addr, vmem_size_t size)
{
	size_t pages = ((size_t)size + PAGE_SIZE - 1) >> PGSHIFT;

	(void)vm;
	fantuan_rump_pages_free((void *)(uintptr_t)addr, pages);
}

/* uvm_km_alloc()/free(): the socket layer's kva loan path calls these; the
 * frame allocator provides the VA-free pages (uvm_km_kmem_alloc already
 * identifies VA and PA here). */
vaddr_t
uvm_km_alloc(struct vm_map *map, vsize_t size, vsize_t align, uvm_flag_t flags)
{
	vmem_addr_t addr;

	(void)map;
	(void)align;
	(void)flags;
	if (uvm_km_kmem_alloc(NULL, size, 0, &addr) != 0)
		return 0;
	return (vaddr_t)addr;
}

void
uvm_km_free(struct vm_map *map, vaddr_t addr, vsize_t size, uvm_flag_t flags)
{

	(void)map;
	(void)flags;
	uvm_km_kmem_free(NULL, (vmem_addr_t)addr, size);
}
