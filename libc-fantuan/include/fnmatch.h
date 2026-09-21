/* libc-fantuan — fnmatch.h (P3): shell pattern matching (src/fnmatch.c). */
#ifndef _FNMATCH_H
#define _FNMATCH_H

#define FNM_NOMATCH 1

#define FNM_PATHNAME (1 << 0)   /* '*' and '?' do not match '/' */
#define FNM_NOESCAPE (1 << 1)   /* backslash is an ordinary character */
#define FNM_PERIOD (1 << 2)     /* leading '.' must be matched literally */
#define FNM_LEADING_DIR (1 << 3) /* pattern matching a path prefix is a match */
#define FNM_CASEFOLD (1 << 4)   /* case-insensitive (GNU/BSD extension) */

int fnmatch(const char *pattern, const char *string, int flags);

#endif /* _FNMATCH_H */
