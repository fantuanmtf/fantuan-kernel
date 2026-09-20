/* rump_tls.h - kernel TLS/HTTPS client interface (ours, M11 R8).
 * The client mirrors the plain HTTP client shape: begin a session over an
 * already-resolved address, set the request, then poll from the net task.
 * Verification is on by default (pinned CA); insecure is only used by the
 * optional external phase through the host relay. */
#ifndef FANTUAN_RUMP_TLS_H
#define FANTUAN_RUMP_TLS_H

#include <sys/types.h>
#include <stdint.h>

/* One-shot bring-up: platform callbacks, CSPRNG seed, CA parse, then KATs. */
int rump_tls_init(void);
int rump_tls_kat(void);
/* Pinned CA length (0 when the build embedded none). */
size_t rump_tls_ca_len(void);
int rump_tls_ready(void);

/* Open a session to HOST at ADDR:PORT.  VERIFY=1 checks the chain and the
 * hostname against the pinned CA; TIMEOUT is in PIT ticks. */
void rump_tls_begin(const char *host, size_t hostlen, uint32_t addr,
    uint16_t port, uint16_t lport, int verify, uint32_t timeout);
/* Request spec: null BODY/CTYPE means GET; capture=1 stores Set-Cookie and
 * the DuckDuckGo x-vqd-4 token from the response headers. */
void rump_tls_request(const char *method, const char *path, const char *ctype,
    const char *body, size_t bodylen, int capture);
void rump_tls_set_cookie(const char *cookie);
void rump_tls_set_vqd(const char *vqd);

int rump_tls_poll(void);		/* 0 running, 1 done, -1 failed */
int rump_tls_status_code(void);
size_t rump_tls_body_len(void);
uint32_t rump_tls_body_hash(void);
const uint8_t *rump_tls_body(size_t *len);
int rump_tls_truncated(void);
const char *rump_tls_cookie(void);
const char *rump_tls_vqd(void);
const char *rump_tls_error(void);

/* Response header helpers (rump_tls_pkt.c). */
int rump_tls_header_find(const uint8_t *buf, size_t len, const char *name,
    const uint8_t **value, size_t *vlen);
int rump_tls_body_has(const uint8_t *buf, size_t len, const char *needle);

#endif
