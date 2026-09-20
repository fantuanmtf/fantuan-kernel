/* rump_http.h - HTTP response helpers (ours, M11 R7).
 * Split from rump_http.c so each adapter file stays inside the size rule. */
#ifndef FANTUAN_RUMP_HTTP_H
#define FANTUAN_RUMP_HTTP_H

#include <sys/types.h>

uint32_t rump_http_fnv1a(const uint8_t *, size_t);
/* Locate the entity body after the CRLFCRLF separator; returns the body
 * pointer (or NULL) and sets *off to its offset in BUF. */
const uint8_t *rump_http_find_body(const uint8_t *, size_t, size_t *);
/* Parsed Content-Length header, or -1 when absent/malformed. */
int rump_http_content_length(const uint8_t *, size_t);
/* Status code of the response line (0 when not an HTTP/x.y line). */
int rump_http_status(const uint8_t *, size_t);

#endif
