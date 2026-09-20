/* libc-fantuan — strings.h (BSD compatibility, P1). */
#ifndef _STRINGS_H
#define _STRINGS_H

#include <stddef.h>

int strcasecmp(const char *s1, const char *s2);
int strncasecmp(const char *s1, const char *s2, size_t n);
int ffs(int i);
void bzero(void *s, size_t n);
void bcopy(const void *src, void *dest, size_t n);
int bcmp(const void *s1, const void *s2, size_t n);
char *index(const char *s, int c);
char *rindex(const char *s, int c);

#endif /* _STRINGS_H */
