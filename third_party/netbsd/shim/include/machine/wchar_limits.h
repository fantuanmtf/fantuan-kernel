/* fantuan adaptation shim: x86_64 wchar/wint limits (not upstream NetBSD). */
#ifndef FANTUAN_MACHINE_WCHAR_LIMITS_H
#define FANTUAN_MACHINE_WCHAR_LIMITS_H
#define WCHAR_MIN (-0x7fffffff - 1)
#define WCHAR_MAX 0x7fffffff
#define WINT_MIN (-0x7fffffff - 1)
#define WINT_MAX 0x7fffffff
#endif
