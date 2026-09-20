/* libc-fantuan — sys/param.h (P1). */
#ifndef _SYS_PARAM_H
#define _SYS_PARAM_H

#include <limits.h>

#define MAXPATHLEN PATH_MAX
#define MAXNAMLEN NAME_MAX
#define NBBY 8
#define DEV_BSIZE 512

#define MIN(a, b) ((a) < (b) ? (a) : (b))
#define MAX(a, b) ((a) > (b) ? (a) : (b))

#endif /* _SYS_PARAM_H */
