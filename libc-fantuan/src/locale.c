/* libc-fantuan — locale/langinfo (P3): the C/POSIX locale only.
 *
 * setlocale accepts "C", "POSIX" and "" (resolved to "C"); any other
 * locale name fails with NULL, which is what an honest minimal libc with
 * no locale database should do. localeconv/nl_langinfo return the C
 * locale's fixed English strings. */
#include <langinfo.h>
#include <locale.h>
#include <string.h>

static const char *const c_abday[7] = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
static const char *const c_day[7] = {"Sunday", "Monday", "Tuesday", "Wednesday",
                                     "Thursday", "Friday", "Saturday"};
static const char *const c_abmon[12] = {"Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"};
static const char *const c_mon[12] = {"January", "February", "March", "April",
                                      "May", "June", "July", "August",
                                      "September", "October", "November", "December"};

char *setlocale(int category, const char *locale)
{
    (void)category;
    if (locale == NULL) {
        return (char *)"C";
    }
    if (locale[0] == '\0' || strcmp(locale, "C") == 0 || strcmp(locale, "POSIX") == 0) {
        return (char *)"C";
    }
    return NULL;
}

struct lconv *localeconv(void)
{
    static struct lconv c_lconv = {
        .decimal_point = (char *)".",
        .thousands_sep = (char *)"",
        .grouping = (char *)"",
        .int_curr_symbol = (char *)"",
        .currency_symbol = (char *)"",
        .mon_decimal_point = (char *)"",
        .mon_thousands_sep = (char *)"",
        .mon_grouping = (char *)"",
        .positive_sign = (char *)"",
        .negative_sign = (char *)"",
        .int_frac_digits = 127,
        .frac_digits = 127,
        .p_cs_precedes = 127,
        .p_sep_by_space = 127,
        .n_cs_precedes = 127,
        .n_sep_by_space = 127,
        .p_sign_posn = 127,
        .n_sign_posn = 127,
        .int_p_cs_precedes = 127,
        .int_p_sep_by_space = 127,
        .int_n_cs_precedes = 127,
        .int_n_sep_by_space = 127,
        .int_p_sign_posn = 127,
        .int_n_sign_posn = 127,
    };
    return &c_lconv;
}

char *nl_langinfo(int item)
{
    switch (item) {
    case CODESET: return (char *)"ANSI_X3.4-1968";
    case D_T_FMT: return (char *)"%a %b %e %H:%M:%S %Y";
    case D_FMT: return (char *)"%m/%d/%y";
    case T_FMT: return (char *)"%H:%M:%S";
    case T_FMT_AMPM: return (char *)"%I:%M:%S %p";
    case AM_STR: return (char *)"AM";
    case PM_STR: return (char *)"PM";
    case RADIXCHAR: return (char *)".";
    case THOUSEP: return (char *)"";
    case YESEXPR: return (char *)"^[yY]";
    case NOEXPR: return (char *)"^[nN]";
    case CRNCYSTR: return (char *)"";
    case ERA: return (char *)"";
    case ERA_D_T_FMT: return (char *)"";
    case ERA_D_FMT: return (char *)"";
    case ERA_T_FMT: return (char *)"";
    case ALT_DIGITS: return (char *)"";
    default:
        if (item >= ABDAY_1 && item <= ABDAY_7) return (char *)c_abday[item - ABDAY_1];
        if (item >= DAY_1 && item <= DAY_7) return (char *)c_day[item - DAY_1];
        if (item >= ABMON_1 && item <= ABMON_12) return (char *)c_abmon[item - ABMON_1];
        if (item >= MON_1 && item <= MON_12) return (char *)c_mon[item - MON_1];
        return (char *)"";
    }
}
