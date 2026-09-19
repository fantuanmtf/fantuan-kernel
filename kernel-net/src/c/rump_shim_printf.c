/* rump_shim_printf.c - minimal formatted output to the kernel log (ours).
 * Only the conversion subset the imported sources and the self-test use:
 * d i u x X c s p and %%, with '-', '0', width and l/ll/z lengths.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/syslog.h>
#include <sys/timevar.h>
#include <stdarg.h>
#include "rump_shim.h"

const char *panicstr;

struct out {
	char *buf;
	size_t size;
	size_t len;
};

static void
out_ch(struct out *o, char c)
{

	if (o->buf != NULL && o->len + 1 < o->size)
		o->buf[o->len] = c;
	o->len++;
}

static void
out_str(struct out *o, const char *s, size_t n)
{

	while (n-- > 0)
		out_ch(o, *s++);
}

static void
out_pad(struct out *o, char c, int n)
{

	while (n-- > 0)
		out_ch(o, c);
}

static void
out_num(struct out *o, unsigned long long v, unsigned base, bool upper,
    int width, int zeropad, int left)
{
	char tmp[32];
	const char *digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
	int n = 0, pad;

	do {
		tmp[n++] = digits[v % base];
		v /= base;
	} while (v != 0);
	pad = width > n ? width - n : 0;
	if (!left)
		out_pad(o, zeropad ? '0' : ' ', pad);
	while (n > 0)
		out_ch(o, tmp[--n]);
	if (left)
		out_pad(o, ' ', pad);
}

static size_t
format(char *buf, size_t size, const char *fmt, va_list ap)
{
	struct out o = { .buf = buf, .size = size, .len = 0 };

	for (; *fmt != '\0'; fmt++) {
		int left = 0, zeropad = 0, width = 0, len = 0;
		unsigned long long v;

		if (*fmt != '%') {
			out_ch(&o, *fmt);
			continue;
		}
		fmt++;
		while (*fmt == '-' || *fmt == '0' || *fmt == '+' || *fmt == ' ') {
			if (*fmt == '-')
				left = 1;
			if (*fmt == '0')
				zeropad = 1;
			fmt++;
		}
		while (*fmt >= '0' && *fmt <= '9')
			width = width * 10 + (*fmt++ - '0');
		while (*fmt == 'l' || *fmt == 'z' || *fmt == 'h') {
			if (*fmt == 'l')
				len++;
			fmt++;
		}
		switch (*fmt) {
		case 'd':
		case 'i':
			if (len >= 2)
				v = (unsigned long long)va_arg(ap, long long);
			else if (len == 1)
				v = (unsigned long long)va_arg(ap, long);
			else
				v = (unsigned long long)va_arg(ap, int);
			if ((long long)v < 0) {
				out_ch(&o, '-');
				v = (unsigned long long)(-(long long)v);
				if (width > 0)
					width--;
			}
			out_num(&o, v, 10, false, width, zeropad, left);
			break;
		case 'u':
		case 'x':
		case 'X':
			if (len >= 2)
				v = va_arg(ap, unsigned long long);
			else if (len == 1)
				v = va_arg(ap, unsigned long);
			else
				v = va_arg(ap, unsigned int);
			out_num(&o, v, *fmt == 'u' ? 10 : 16, *fmt == 'X',
			    width, zeropad, left);
			break;
		case 'p': {
			void *p = va_arg(ap, void *);
			out_str(&o, "0x", 2);
			out_num(&o, (unsigned long long)(uintptr_t)p, 16, false,
			    0, 0, 0);
			break;
		}
		case 's': {
			const char *s = va_arg(ap, const char *);
			size_t n;

			if (s == NULL)
				s = "(null)";
			n = strlen(s);
			if (width > 0 && (int)n < width && !left)
				out_pad(&o, ' ', width - (int)n);
			out_str(&o, s, n);
			if (width > 0 && (int)n < width && left)
				out_pad(&o, ' ', width - (int)n);
			break;
		}
		case 'c':
			out_ch(&o, (char)va_arg(ap, int));
			break;
		case '%':
			out_ch(&o, '%');
			break;
		default:
			out_ch(&o, '%');
			if (*fmt != '\0')
				out_ch(&o, *fmt);
			break;
		}
	}
	if (o.buf != NULL && o.size != 0)
		o.buf[o.len < o.size ? o.len : o.size - 1] = '\0';
	return o.len;
}

int
snprintf(char *buf, size_t size, const char *fmt, ...)
{
	va_list ap;
	int rv;

	va_start(ap, fmt);
	rv = (int)format(buf, size, fmt, ap);
	va_end(ap);
	return rv;
}

int
vsnprintf(char *buf, size_t size, const char *fmt, va_list ap)
{

	return (int)format(buf, size, fmt, ap);
}

void
printf(const char *fmt, ...)
{
	char buf[1024];
	va_list ap;

	va_start(ap, fmt);
	(void)format(buf, sizeof(buf), fmt, ap);
	va_end(ap);
	fantuan_rump_log(buf, strlen(buf));
}

void
log(int level, const char *fmt, ...)
{
	char buf[1024];
	va_list ap;

	(void)level;
	va_start(ap, fmt);
	(void)format(buf, sizeof(buf), fmt, ap);
	va_end(ap);
	fantuan_rump_log(buf, strlen(buf));
}

void
panic(const char *fmt, ...)
{
	char buf[1024];
	va_list ap;
	size_t n;

	n = strlen("rump: panic: ");
	memcpy(buf, "rump: panic: ", n);
	va_start(ap, fmt);
	(void)format(buf + n, sizeof(buf) - n, fmt, ap);
	va_end(ap);
	buf[sizeof(buf) - 1] = '\0';
	panicstr = buf;
	fantuan_rump_panic(buf, strlen(buf));
}

void
kern_assert(const char *fmt, ...)
{
	char buf[512];
	va_list ap;

	va_start(ap, fmt);
	(void)format(buf, sizeof(buf), fmt, ap);
	va_end(ap);
	panic("%s", buf);
}

int
ratecheck(struct timeval *last, const struct timeval *interval)
{
	time_t now = time_uptime;

	if (last == NULL || interval == NULL)
		return 0;
	if (now - last->tv_sec >= interval->tv_sec) {
		last->tv_sec = now;
		last->tv_usec = 0;
		return 1;
	}
	return 0;
}
