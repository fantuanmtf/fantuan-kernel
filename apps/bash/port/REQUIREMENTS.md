# bash minimal libc/POSIX surface (C5; landed in P3)

What GNU Bash 5.3 needs to compile and run over the fantuan native ABI.
Grouped by subsystem; the C5 spike probes this list against the bare target
(`tools/build-bash-spike.sh`, 102 symbols / 43 headers) and records the
missing ones. `posix-libc` means this whole surface once M14-2/M14-3 provide
the syscalls and M14-4 ports musl.

**P3 status (2026-09): all of it resolves against libc-fantuan** - the spike
reports 0/43 missing headers and 102/102 probed symbols provided. The
per-item notes below record where the P3 implementations live or where a
stub is deliberate (`forkpty`/`openpty`, `dl*`, `iconv`); see
`../../libc-fantuan/README.md` and `README.md` here.

## Startup and process

- `_start`/crt0, `exit`/`_exit`, `main(argc, argv, envp)`;
- `fork`, `execve`, `waitpid`, `getpid`, `getppid`, `getuid`, `geteuid`,
  `getgid`, `getegid`, `setpgid`, `umask`;
- `kill`, `killpg`, `_exit` on fatal shell errors.

## Memory

- `malloc`/`calloc`/`realloc`/`free` (bash is built `--without-bash-malloc`,
  so the libc allocator must work);
- `mmap`/`munmap` may be needed by the allocator's large-block path.

## Files, directories, descriptors

- `open`, `close`, `read`, `write`, `lseek`, `fcntl`, `dup`, `dup2`, `pipe`;
- `stat`, `lstat`, `fstat`, `access`, `readlink`, `symlink`, `unlink`,
  `rename`, `mkdir`, `rmdir`, `chdir`, `getcwd`;
- `opendir`, `readdir`, `closedir`, `dirfd`;
- buffered stdio: `fopen`, `fdopen`, `fclose`, `fread`, `fwrite`, `fflush`,
  `fgetc`/`getc`, `fgets`, `getline`, `puts`, `printf`/`fprintf`/
  `snprintf`/`vsnprintf` and the `vfprintf` core;
- `errno` with the common `E*` values and `strerror`.

## Pipes, job control, terminal

- `pipe`, `fork`/`execve`/`waitpid` semantics with process groups;
- `tcgetpgrp`, `tcsetpgrp`, `setpgid`, `killpg`;
- `isatty`, `ttyname`, `tcgetattr`, `tcsetattr`, `termios` structs and the
  `ICANON`/`ECHO`/`ISIG` flags;
- optional `openpty`, `forkpty` for the `--enable-net-redirections`/pty
  paths (can stay unset until readline/expect work).

## Signals

- `signal`, `sigaction`, `sigprocmask`, `sigsuspend`, `sigemptyset`,
  `sigaddset`, `sigfillset`, `sigismember`;
- a `sig_atomic_t` and a working `SIGCHLD`/`SIGINT`/`SIGTERM`/`SIGQUIT`
  delivery path (trap, wait-for-job, `SIGWINCH` optional).

## Identity and environment

- `getpwnam`, `getpwuid`, `getgrnam`, `getgrgid`, `endpwent`/`endgrent`
  (prompt `\u`, `~\` expansion, `ulimit` messages);
- `getenv`, `putenv`, `setenv`, `unsetenv`;
- `getrlimit`, `setrlimit` (`ulimit` builtin).

## Expansion and patterns

- `glob`, `globfree`, `fnmatch` (pathname expansion);
- `wordexp`, `wordfree` (`complete`/`readline` integration, optional);
- `regcomp`, `regexec`, `regfree` (`=~`, `[[ ]]` pattern matching);
- `mbsrtowcs`, `wcsrtombs`, `mbrtowc`, `wcrtomb`, `wcslen`, `wcwidth`
  (multibyte/wide-char handling; ASCII-only stubs are acceptable at first).

## Locale and time

- `setlocale`, `localeconv`, `nl_langinfo` (locale stubs: `C`/`POSIX` only
  until a full locale database exists; bash must degrade gracefully);
- `time`, `gettimeofday`, `localtime`, `gmtime`, `mktime`, `strftime`,
  `strptime`, `tzset` (`history` timestamps, `printf %(%T)T`);
- `nanosleep`, `alarm`, `setitimer` (timeouts, `read -t`).

## Diagnostics and misc

- `sysconf`, `pathconf`, `getrusage`, `times`, `poll`, `select`, `ioctl`;
- `dlopen`/`dlsym`/`dlclose` (loadable builtins; may be compiled out);
- `system`, `popen`, `pclose` (subshell command substitution paths).

## Headers

`stdio.h stdlib.h string.h strings.h unistd.h fcntl.h errno.h signal.h
setjmp.h inttypes.h math.h sys/types.h sys/stat.h sys/wait.h sys/times.h
sys/resource.h sys/time.h sys/select.h sys/socket.h sys/ioctl.h sys/file.h
sys/param.h termios.h pwd.h grp.h dirent.h time.h wchar.h wctype.h glob.h
locale.h langinfo.h iconv.h wordexp.h poll.h dlfcn.h paths.h libintl.h
regex.h` (plus the compiler-provided `stdarg.h stddef.h stdint.h limits.h`).

## Explicit non-goals for the first port

- NLS/gettext (`--disable-nls`), readline (`--without-readline`), malloc
  replacement (`--without-bash-malloc`);
- full locale databases and iconv tables: stubs for the `C` locale first,
  with honest failures for unsupported conversions;
- threads: bash 5.3 has no thread dependency on this path.
