/* libc-fantuan — environment (P1).
 *
 * Split from stdlib.c to keep files within the repo's 300-line convention.
 * The static table replaces the single kernel-supplied (empty) environ
 * block once setenv/putenv is called. */
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

char *getenv(const char *name)
{
    extern char **environ;
    size_t len = strlen(name);
    if (!environ) {
        return NULL;
    }
    for (char **e = environ; *e; e++) {
        if (strncmp(*e, name, len) == 0 && (*e)[len] == '=') {
            return *e + len + 1;
        }
    }
    return NULL;
}

/* P1 environment store: a small static table; putenv/setenv allocate. */
#define ENV_MAX 32
static char *envtab[ENV_MAX];
static int envtab_used;

static int env_set(const char *name, const char *value)
{
    size_t nl = strlen(name);
    if (nl == 0 || strchr(name, '=')) {
        errno = EINVAL;
        return -1;
    }
    size_t vl = value ? strlen(value) : 0;
    char *entry = malloc(nl + vl + 2);
    if (!entry) {
        return -1;
    }
    memcpy(entry, name, nl);
    entry[nl] = '=';
    if (value) {
        memcpy(entry + nl + 1, value, vl);
    }
    entry[nl + 1 + vl] = 0;
    /* Replace an existing name in place; else append. */
    for (int i = 0; i < envtab_used; i++) {
        if (strncmp(envtab[i], name, nl) == 0 && envtab[i][nl] == '=') {
            envtab[i] = entry;
            return 0;
        }
    }
    if (envtab_used >= ENV_MAX - 1) {
        errno = ENOMEM;
        return -1;
    }
    envtab[envtab_used++] = entry;
    envtab[envtab_used] = NULL;
    environ = envtab;
    return 0;
}

int setenv(const char *name, const char *value, int overwrite)
{
    if (!overwrite && getenv(name)) {
        return 0;
    }
    return env_set(name, value);
}

int unsetenv(const char *name)
{
    size_t nl = strlen(name);
    if (nl == 0 || strchr(name, '=')) {
        errno = EINVAL;
        return -1;
    }
    for (int i = 0; i < envtab_used; i++) {
        if (strncmp(envtab[i], name, nl) == 0 && envtab[i][nl] == '=') {
            for (int j = i; j < envtab_used; j++) {
                envtab[j] = envtab[j + 1];
            }
            envtab_used--;
            return 0;
        }
    }
    return 0;
}

int putenv(char *string)
{
    char *eq = strchr(string, '=');
    if (!eq) {
        return unsetenv(string);
    }
    *eq = 0;
    int r = env_set(string, eq + 1);
    *eq = '=';
    return r;
}
