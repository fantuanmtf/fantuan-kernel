/* libc-fantuan — dlfcn.h (P3): linkable stubs. Fantuan has no dynamic
 * loader (every program is a static ET_EXEC), so dlopen fails cleanly and
 * dlerror explains it; bash's loadable-builtin support is off. */
#ifndef _DLFCN_H
#define _DLFCN_H

#define RTLD_LAZY 0x00001
#define RTLD_NOW 0x00002
#define RTLD_GLOBAL 0x00100
#define RTLD_LOCAL 0
#define RTLD_NOLOAD 0x00004
#define RTLD_DEFAULT ((void *)0)
#define RTLD_NEXT ((void *)-1)

void *dlopen(const char *filename, int flags);
void *dlsym(void *handle, const char *symbol);
int dlclose(void *handle);
char *dlerror(void);

#endif /* _DLFCN_H */
