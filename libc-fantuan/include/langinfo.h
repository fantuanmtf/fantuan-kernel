/* libc-fantuan — langinfo.h (P3): nl_langinfo over the C locale. */
#ifndef _LANGINFO_H
#define _LANGINFO_H

/* Item numbers are private to libc-fantuan; callers only pass the names. */
#define CODESET 0
#define D_T_FMT 1
#define D_FMT 2
#define T_FMT 3
#define T_FMT_AMPM 4
#define AM_STR 5
#define PM_STR 6
#define ABDAY_1 7
#define ABDAY_2 8
#define ABDAY_3 9
#define ABDAY_4 10
#define ABDAY_5 11
#define ABDAY_6 12
#define ABDAY_7 13
#define DAY_1 14
#define DAY_2 15
#define DAY_3 16
#define DAY_4 17
#define DAY_5 18
#define DAY_6 19
#define DAY_7 20
#define ABMON_1 21
#define ABMON_2 22
#define ABMON_3 23
#define ABMON_4 24
#define ABMON_5 25
#define ABMON_6 26
#define ABMON_7 27
#define ABMON_8 28
#define ABMON_9 29
#define ABMON_10 30
#define ABMON_11 31
#define ABMON_12 32
#define MON_1 33
#define MON_2 34
#define MON_3 35
#define MON_4 36
#define MON_5 37
#define MON_6 38
#define MON_7 39
#define MON_8 40
#define MON_9 41
#define MON_10 42
#define MON_11 43
#define MON_12 44
#define ERA 45
#define ERA_D_T_FMT 46
#define ERA_D_FMT 47
#define ERA_T_FMT 48
#define ALT_DIGITS 49
#define RADIXCHAR 50
#define THOUSEP 51
#define YESEXPR 52
#define NOEXPR 53
#define CRNCYSTR 54

char *nl_langinfo(int item);

#endif /* _LANGINFO_H */
