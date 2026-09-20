/* libc-fantuan — time (P1): no RTC, so CLOCK_REALTIME == uptime and all
 * calendar conversions run in UTC. */
#include <errno.h>
#include <string.h>
#include <sys/time.h>
#include <time.h>
#include <fantuan/abi.h>

long timezone;
int daylight;
char *tzname[2] = { (char *)"UTC", (char *)"UTC" };

int clock_gettime(int clockid, struct timespec *tp)
{
    long r = __fantuan_syscall6(FANTUAN_SYS_CLOCK_GETTIME, clockid, (long)tp, 0, 0, 0);
    if (r < 0) {
        errno = (int)-r;
        return -1;
    }
    return 0;
}

time_t time(time_t *tloc)
{
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return (time_t)-1;
    }
    if (tloc) {
        *tloc = ts.tv_sec;
    }
    return ts.tv_sec;
}

int gettimeofday(struct timeval *tv, struct timezone *tz)
{
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return -1;
    }
    if (tv) {
        tv->tv_sec = ts.tv_sec;
        tv->tv_usec = ts.tv_nsec / 1000;
    }
    if (tz) {
        tz->tz_minuteswest = 0;
        tz->tz_dsttime = 0;
    }
    return 0;
}

int settimeofday(const struct timeval *tv, const struct timezone *tz)
{
    (void)tv;
    (void)tz;
    errno = ENOSYS;
    return -1;
}

clock_t clock(void)
{
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return (clock_t)-1;
    }
    return (clock_t)(ts.tv_sec * CLOCKS_PER_SEC + ts.tv_nsec / (1000000000L / CLOCKS_PER_SEC));
}

int nanosleep(const struct timespec *req, struct timespec *rem)
{
    (void)rem;
    if (!req || req->tv_nsec < 0 || req->tv_nsec >= 1000000000L) {
        errno = EINVAL;
        return -1;
    }
    unsigned long ms = (unsigned long)req->tv_sec * 1000UL +
                       (unsigned long)req->tv_nsec / 1000000UL;
    if (ms == 0 && (req->tv_sec > 0 || req->tv_nsec > 0)) {
        ms = 1;
    }
    __fantuan_syscall6(FANTUAN_SYS_SLEEP_MS, (long)ms, 0, 0, 0, 0);
    return 0;
}

unsigned int sleep(unsigned int seconds)
{
    struct timespec ts = { (time_t)seconds, 0 };
    if (nanosleep(&ts, NULL) != 0) {
        return seconds;
    }
    return 0;
}

int usleep(unsigned int usec)
{
    struct timespec ts = { (time_t)(usec / 1000000U), (long)(usec % 1000000U) * 1000L };
    return nanosleep(&ts, NULL);
}

/* Days from civil (Howard Hinnant): 1970-01-01 is day 0. */
static long days_from_civil(long y, unsigned m, unsigned d)
{
    y -= m <= 2;
    long era = (y >= 0 ? y : y - 399) / 400;
    unsigned yoe = (unsigned)(y - era * 400);
    unsigned doy = (153 * (m + (m > 2 ? -3 : 9)) + 2) / 5 + d - 1;
    unsigned doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097L + (long)doe - 719468L;
}

static void civil_from_days(long z, long *y, unsigned *m, unsigned *d)
{
    z += 719468;
    long era = (z >= 0 ? z : z - 146096) / 146097;
    unsigned doe = (unsigned)(z - era * 146097);
    unsigned yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    long yy = (long)yoe + era * 400;
    unsigned doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    unsigned mp = (5 * doy + 2) / 153;
    *d = doy - (153 * mp + 2) / 5 + 1;
    *m = mp + (mp < 10 ? 3 : -9);
    *y = yy + (*m <= 2);
}

static struct tm tm_storage;

static struct tm *fill_tm(time_t t)
{
    long days = t / 86400;
    long secs = t % 86400;
    if (secs < 0) {
        secs += 86400;
        days--;
    }
    long y;
    unsigned m, d;
    civil_from_days(days, &y, &m, &d);
    tm_storage.tm_year = (int)y - 1900;
    tm_storage.tm_mon = (int)m - 1;
    tm_storage.tm_mday = (int)d;
    tm_storage.tm_hour = (int)(secs / 3600);
    tm_storage.tm_min = (int)((secs % 3600) / 60);
    tm_storage.tm_sec = (int)(secs % 60);
    tm_storage.tm_wday = (int)((days + 4) % 7 + 7) % 7;
    tm_storage.tm_yday = (int)(days - days_from_civil(y, 1, 1));
    tm_storage.tm_isdst = 0;
    return &tm_storage;
}

struct tm *gmtime(const time_t *timep) { return fill_tm(*timep); }
struct tm *localtime(const time_t *timep) { return fill_tm(*timep); }

struct tm *gmtime_r(const time_t *timep, struct tm *result)
{
    *result = *fill_tm(*timep);
    return result;
}

struct tm *localtime_r(const time_t *timep, struct tm *result) { return gmtime_r(timep, result); }

time_t mktime(struct tm *tm)
{
    long days = days_from_civil(tm->tm_year + 1900, (unsigned)tm->tm_mon + 1,
                                (unsigned)tm->tm_mday);
    return (time_t)(days * 86400 + tm->tm_hour * 3600 + tm->tm_min * 60 + tm->tm_sec);
}

size_t strftime(char *s, size_t max, const char *format, const struct tm *tm)
{
    static const char *const wday[] = { "Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat" };
    static const char *const mon[] = { "Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                       "Jul", "Aug", "Sep", "Oct", "Nov", "Dec" };
    size_t n = 0;
    char tmp[32];
    for (; *format; format++) {
        if (*format != '%') {
            if (n + 1 >= max) {
                return 0;
            }
            s[n++] = *format;
            continue;
        }
        format++;
        int v = -1;
        const char *lit = NULL;
        switch (*format) {
        case 'Y': v = tm->tm_year + 1900; break;
        case 'y': v = (tm->tm_year + 1900) % 100; break;
        case 'm': v = tm->tm_mon + 1; break;
        case 'd': v = tm->tm_mday; break;
        case 'e': v = tm->tm_mday; break;
        case 'H': v = tm->tm_hour; break;
        case 'M': v = tm->tm_min; break;
        case 'S': v = tm->tm_sec; break;
        case 'j': v = tm->tm_yday + 1; break;
        case 'a': lit = wday[((tm->tm_wday % 7) + 7) % 7]; break;
        case 'A': lit = wday[((tm->tm_wday % 7) + 7) % 7]; break;
        case 'b': lit = mon[((tm->tm_mon % 12) + 12) % 12]; break;
        case 'h': lit = mon[((tm->tm_mon % 12) + 12) % 12]; break;
        case 'p': lit = tm->tm_hour < 12 ? "AM" : "PM"; break;
        case 'Z': lit = "UTC"; break;
        case '%': lit = "%"; break;
        case 0: return n;
        default:
            if (n + 2 >= max) {
                return 0;
            }
            s[n++] = '%';
            s[n++] = *format;
            continue;
        }
        if (lit) {
            for (const char *p = lit; *p; p++) {
                if (n + 1 >= max) {
                    return 0;
                }
                s[n++] = *p;
            }
            continue;
        }
        int len = 0;
        int neg = v < 0;
        unsigned uv = neg ? (unsigned)-v : (unsigned)v;
        do {
            tmp[len++] = (char)('0' + uv % 10);
            uv /= 10;
        } while (uv);
        if (neg) {
            tmp[len++] = '-';
        }
        if (n + (size_t)len + 1 >= max) {
            return 0;
        }
        while (len > 0) {
            s[n++] = tmp[--len];
        }
    }
    s[n] = 0;
    return n;
}

char *strptime(const char *s, const char *format, struct tm *tm)
{
    /* Minimal ISO-ish parser: %Y %m %d %H %M %S and literal separators. */
    for (; *format; format++) {
        if (*format != '%') {
            if (*s != *format) {
                return NULL;
            }
            s++;
            continue;
        }
        format++;
        int *field = NULL;
        int width = 2;
        switch (*format) {
        case 'Y': field = &tm->tm_year; width = 4; break;
        case 'm': field = &tm->tm_mon; break;
        case 'd': field = &tm->tm_mday; break;
        case 'H': field = &tm->tm_hour; break;
        case 'M': field = &tm->tm_min; break;
        case 'S': field = &tm->tm_sec; break;
        default: return NULL;
        }
        int val = 0, digits = 0;
        while (digits < width && *s >= '0' && *s <= '9') {
            val = val * 10 + (*s++ - '0');
            digits++;
        }
        if (digits == 0) {
            return NULL;
        }
        if (*format == 'Y') {
            val -= 1900;
        } else if (*format == 'm') {
            val -= 1;
        }
        *field = val;
    }
    return (char *)s;
}

void tzset(void)
{
    timezone = 0;
    daylight = 0;
}

char *asctime(const struct tm *tm)
{
    static char buf[32];
    strftime(buf, sizeof(buf), "%a %b %e %H:%M:%S %Y", tm);
    return buf;
}

char *ctime(const time_t *timep)
{
    return asctime(localtime(timep));
}
