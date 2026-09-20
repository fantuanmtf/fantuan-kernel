/* rump_tls_pkt.c - TLS/HTTPS response header helpers (ours, M11 R8).
 * Header-block lookup and capture of the two things the external Duck.AI
 * best-effort round needs: the session cookie (Set-Cookie) and the
 * DuckDuckGo x-vqd-4 token.  Case-insensitive, bounded, no allocation. */
#include <sys/types.h>
#include <sys/param.h>
#include "rump_shim.h"
#include "rump_tls.h"

#include <string.h>

static int
ci_equal(const uint8_t *a, const char *b, size_t n)
{
	size_t i;

	for (i = 0; i < n; i++) {
		uint8_t x = a[i], y = (uint8_t)b[i];

		if (x >= 'A' && x <= 'Z')
			x = (uint8_t)(x - 'A' + 'a');
		if (y >= 'A' && y <= 'Z')
			y = (uint8_t)(y - 'A' + 'a');
		if (x != y)
			return 0;
	}
	return 1;
}

int
rump_tls_header_find(const uint8_t *buf, size_t len, const char *name,
    const uint8_t **value, size_t *vlen)
{
	size_t i = 0, nlen = strlen(name);

	while (i < len) {
		size_t eol = i;

		while (eol < len && buf[eol] != '\n')
			eol++;
		if (eol - i > nlen + 1 && buf[i + nlen] == ':' &&
		    ci_equal(buf + i, name, nlen)) {
			size_t v = i + nlen + 1, end = eol;

			while (v < end && (buf[v] == ' ' || buf[v] == '\t'))
				v++;
			while (end > v && (buf[end - 1] == '\r' ||
			    buf[end - 1] == ' '))
				end--;
			if (value != NULL)
				*value = buf + v;
			if (vlen != NULL)
				*vlen = end - v;
			return 0;
		}
		i = eol + 1;
		if (i == len)
			break;
	}
	return -1;
}

int
rump_tls_body_has(const uint8_t *buf, size_t len, const char *needle)
{
	size_t n = strlen(needle), i;

	if (n == 0 || n > len)
		return 0;
	for (i = 0; i + n <= len; i++)
		if (ci_equal(buf + i, needle, n))
			return 1;
	return 0;
}

void
rump_tls_capture(const uint8_t *hdr, size_t len)
{
	const uint8_t *v;
	size_t vlen, n;
	char tmp[257];

	if (rump_tls_header_find(hdr, len, "x-vqd-4", &v, &vlen) == 0 &&
	    vlen > 0) {
		n = vlen < sizeof(tmp) - 1 ? vlen : sizeof(tmp) - 1;
		memcpy(tmp, v, n);
		tmp[n] = '\0';
		rump_tls_set_vqd(tmp);
	}
	if (rump_tls_header_find(hdr, len, "set-cookie", &v, &vlen) == 0 &&
	    vlen > 0) {
		for (n = 0; n < vlen && v[n] != ';' && n < sizeof(tmp) - 1; n++)
			tmp[n] = (char)v[n];
		tmp[n] = '\0';
		rump_tls_set_cookie(tmp);
	}
}
