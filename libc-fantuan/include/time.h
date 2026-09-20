/* libc-fantuan — time.h (P1): no RTC, so CLOCK_REALTIME == monotonic since
 * boot (documented). mktime/strftime are minimal civil-time conversions. */
#ifndef _TIME_H
#define _TIME_H

#include <stddef.h>
#include <sys/types.h>

#define CLOCKS_PER_SEC 100
#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1

struct timespec {
    time_t tv_sec;
    long tv_nsec;
};

struct tm {
    int tm_sec;
    int tm_min;
    int tm_hour;
    int tm_mday;
    int tm_mon;
    int tm_year;
    int tm_wday;
    int tm_yday;
    int tm_isdst;
};

time_t time(time_t *tloc);
int clock_gettime(int clockid, struct timespec *tp);
int nanosleep(const struct timespec *req, struct timespec *rem);
clock_t clock(void);
struct tm *localtime(const time_t *timep);
struct tm *gmtime(const time_t *timep);
struct tm *localtime_r(const time_t *timep, struct tm *result);
struct tm *gmtime_r(const time_t *timep, struct tm *result);
time_t mktime(struct tm *tm);
size_t strftime(char *s, size_t max, const char *format, const struct tm *tm);
char *strptime(const char *s, const char *format, struct tm *tm);
void tzset(void);
char *ctime(const time_t *timep);
char *asctime(const struct tm *tm);

extern long timezone;
extern int daylight;
extern char *tzname[2];

#endif /* _TIME_H */
