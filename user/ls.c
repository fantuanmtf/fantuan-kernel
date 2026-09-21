/* libc-fantuan — ls (P2): minimal directory lister for the dash userland.
 *
 * Usage: ls [-l] [path...]
 * Lists directory entries one per line; -l prefixes the st_mode string,
 * size and name like the classic long format. The first-party tool exists
 * so P2 can prove dash's fork/exec/pipe path against a real /bin program
 * (there is no coreutils in the image). */
#include <dirent.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>

static void mode_string(mode_t mode, char out[11])
{
    out[0] = S_ISDIR(mode) ? 'd' : (S_ISCHR(mode) ? 'c' : (S_ISFIFO(mode) ? 'p' : '-'));
    const char *rwx = "rwxrwxrwx";
    for (int i = 0; i < 9; i++) {
        out[1 + i] = (mode & (1u << (8 - i))) ? rwx[i] : '-';
    }
    out[10] = 0;
}

static void list_one(const char *path, int long_fmt)
{
    struct stat st;
    if (stat(path, &st) != 0) {
        fprintf(stderr, "ls: %s: cannot stat\n", path);
        return;
    }
    if (!S_ISDIR(st.st_mode)) {
        if (long_fmt) {
            char m[11];
            mode_string(st.st_mode, m);
            printf("%s %8lu %s\n", m, (unsigned long)st.st_size, path);
        } else {
            printf("%s\n", path);
        }
        return;
    }

    DIR *dir = opendir(path);
    if (!dir) {
        fprintf(stderr, "ls: %s: cannot open\n", path);
        return;
    }
    int base = 0;
    if (path[0] == '/' && path[1] == 0) {
        base = 1; /* joining under "/" must not produce "//name" */
    }
    struct dirent *de;
    while ((de = readdir(dir)) != NULL) {
        if (strcmp(de->d_name, ".") == 0 || strcmp(de->d_name, "..") == 0) {
            continue;
        }
        if (!long_fmt) {
            printf("%s\n", de->d_name);
            continue;
        }
        char full[256];
        snprintf(full, sizeof full, "%s%s%s", path, base ? "" : "/", de->d_name);
        struct stat est;
        if (stat(full, &est) != 0) {
            memset(&est, 0, sizeof est);
        }
        char m[11];
        mode_string(est.st_mode, m);
        printf("%s %8lu %s\n", m, (unsigned long)est.st_size, de->d_name);
    }
    closedir(dir);
}

int main(int argc, char **argv)
{
    int long_fmt = 0;
    int first = 1;
    for (int i = 1; i < argc && argv[i][0] == '-' && argv[i][1] != 0; i++) {
        if (strcmp(argv[i], "-l") == 0) {
            long_fmt = 1;
            first = i + 1;
        } else {
            fprintf(stderr, "ls: unknown option %s\n", argv[i]);
            return 2;
        }
    }
    if (first >= argc) {
        list_one(".", long_fmt);
        return 0;
    }
    for (int i = first; i < argc; i++) {
        list_one(argv[i], long_fmt);
    }
    return 0;
}
