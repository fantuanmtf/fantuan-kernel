/* rump_http_pkt.c - HTTP/1.x response parsing helpers (ours, M11 R7). */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_http.h"

uint32_t
rump_http_fnv1a(const uint8_t *p, size_t n)
{
	uint32_t h = 2166136261u;

	while (n-- > 0) {
		h ^= *p++;
		h *= 16777619u;
	}
	return h;
}

const uint8_t *
rump_http_find_body(const uint8_t *buf, size_t len, size_t *off)
{
	static const char sep[] = "\r\n\r\n";
	size_t i;

	for (i = 0; i + sizeof(sep) - 1 <= len; i++)
		if (memcmp(buf + i, sep, sizeof(sep) - 1) == 0) {
			*off = i + sizeof(sep) - 1;
			return buf + *off;
		}
	return NULL;
}

int
rump_http_content_length(const uint8_t *buf, size_t hlen)
{
	static const char key[] = "content-length:";
	size_t i, j;

	for (i = 0; i + sizeof(key) - 1 <= hlen; i++) {
		int match = 1;

		for (j = 0; j < sizeof(key) - 1; j++) {
			char c = (char)buf[i + j];

			if (c >= 'A' && c <= 'Z')
				c = (char)(c - 'A' + 'a');
			if (c != key[j]) {
				match = 0;
				break;
			}
		}
		if (!match)
			continue;
		i += sizeof(key) - 1;
		while (i < hlen && (buf[i] == ' ' || buf[i] == '\t'))
			i++;
		{
			int v = 0;

			while (i < hlen && buf[i] >= '0' && buf[i] <= '9')
				v = v * 10 + (buf[i++] - '0');
			return v;
		}
	}
	return -1;
}

int
rump_http_status(const uint8_t *buf, size_t len)
{
	size_t i = 0;
	int v = 0;

	if (len < 12 || memcmp(buf, "HTTP/", 5) != 0)
		return 0;
	while (i < len && buf[i] != ' ')
		i++;
	while (i < len && buf[i] == ' ')
		i++;
	while (i < len && buf[i] >= '0' && buf[i] <= '9' && v < 1000)
		v = v * 10 + (buf[i++] - '0');
	return v;
}
