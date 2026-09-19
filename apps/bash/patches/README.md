# bash portability patches

Empty until M14-8. The plan for the musl/fantuan-ABI port:

- keep `src/bash-5.3.tar.gz` pristine; patches are replayed on top at build
  time and every file added here is listed in the manifest `patches` array;
- the expected shape is small: a `config.sub`/toolchain hook only if the
  fantuan target needs one, musl-compatible build flags (`--without-bash-malloc`
  so malloc comes from musl), job-control/pty and terminal defaults over the
  native ABI, and minimal locale/`/etc` assumptions;
- no patch may pull bash code into the kernel or base libraries; bash stays a
  separate executable linked only against the userland C library (M14-4/M14-8).

`.gitkeep` keeps this directory present while it is empty.
