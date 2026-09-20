/* rump_ext.c - optional external TCP/TLS phase (ours, M11 R8).
 * Runs only when the guest has a lease; every target is reached through the
 * host relay (tools/tor_relay.py) at 10.0.2.2:19050, which speaks SOCKS5 to
 * the local Tor/I2P proxy and resolves the hostname proxy-side (socks5h), so
 * DNS pollution cannot turn a reachable target into a failure.  All checks
 * are best-effort: when the relay is absent the phase prints one
 * `net: ext skip (no relay)` line and stops; individual failures are
 * recorded, never gated.  Tor/I2P carry TCP only, so there is no UDP here
 * (the host UDP echo test is the UDP coverage).
 *
 * rungs: github.com (status/body), x.com (liveness), duckduckgo.com (HTML
 * marker + cookie), Duck.AI best-effort round (status token + cookie + one
 * POST; a non-empty response body is the strongest claim without JS). */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_shim.h"
#include "rump_tls.h"
#include "rump_ext.h"

#define EXT_RELAY_ADDR	0x0a000202u	/* 10.0.2.2 */
#define EXT_RELAY_PORT	19050
#define EXT_LPORT	41950
#define EXT_TIMEOUT	2000		/* PIT ticks (100 Hz): 20 s */

#define DUCKAI_PATH	"/duckchat/v1/chat"
#define DUCKAI_STATUS	"/duckchat/v1/status"
#define DUCKAI_BODY	"{\"model\":\"gpt-4o-mini\",\"messages\":" \
			"[{\"role\":\"user\",\"content\":\"ping\"}]}"

enum {
	EXT_IDLE, EXT_GITHUB, EXT_X, EXT_DDG, EXT_DDG_STATUS, EXT_DDG_CHAT,
	EXT_DONE
};

static int ext_state;
static int ext_skipped;

static void
ext_start(int state, const char *host, size_t hostlen, const char *method,
    const char *path, const char *ctype, const char *body, size_t bodylen,
    int capture)
{

	ext_state = state;
	rump_tls_request(method, path, ctype, body, bodylen, capture);
	rump_tls_begin(host, hostlen, EXT_RELAY_ADDR, EXT_RELAY_PORT,
	    (uint16_t)(EXT_LPORT + state), 0, EXT_TIMEOUT);
}

void
rump_ext_begin(void)
{

	ext_skipped = 0;
	if (!rump_tls_ready() || rump_tls_ca_len() == 0) {
		printf("net: ext skip (no tls)\n");
		ext_state = EXT_DONE;
		return;
	}
	rump_tls_set_vqd("");
	rump_tls_set_cookie("");
	ext_start(EXT_GITHUB, "github.com", 10, "GET", "/", NULL, NULL, 0, 0);
}

/* Advance the current session: 0 running, 1 done-ok, -1 done-failed
 * (marker already printed), 2 skipped (relay absent, phase over). */
static int
ext_advance(const char *name)
{
	int r = rump_tls_poll();

	if (r == 0)
		return 0;
	if (r < 0) {
		if (!ext_skipped && strcmp(rump_tls_error(), "connect") == 0 &&
		    ext_state == EXT_GITHUB) {
			printf("net: ext skip (no relay)\n");
			ext_skipped = 1;
			ext_state = EXT_DONE;
			return 2;
		}
		printf("net: ext %s FAIL (%s)\n", name, rump_tls_error());
		return -1;
	}
	return 1;
}

int
rump_ext_poll(void)
{
	int adv, status;
	size_t bytes;

	if (ext_state == EXT_IDLE || ext_state == EXT_DONE)
		return 1;

	switch (ext_state) {
	case EXT_GITHUB:
		adv = ext_advance("github");
		if (adv == 0)
			return 0;
		if (adv == 2)
			return 1;
		if (adv > 0) {
			status = rump_tls_status_code();
			bytes = rump_tls_body_len();
			if (status > 0 && bytes > 0)
				printf("net: ext github ok (status=%d "
				    "bytes=%lu)\n", status,
				    (unsigned long)bytes);
			else
				printf("net: ext github FAIL (status=%d "
				    "bytes=%lu)\n", status,
				    (unsigned long)bytes);
		}
		ext_start(EXT_X, "x.com", 5, "GET", "/", NULL, NULL, 0, 0);
		return 0;
	case EXT_X:
		adv = ext_advance("x");
		if (adv == 0)
			return 0;
		if (adv > 0) {
			status = rump_tls_status_code();
			bytes = rump_tls_body_len();
			if (status > 0)
				printf("net: ext x ok (status=%d bytes=%lu)\n",
				    status, (unsigned long)bytes);
			else
				printf("net: ext x FAIL (status=%d bytes=%lu)"
				    "\n", status, (unsigned long)bytes);
		}
		ext_start(EXT_DDG, "duckduckgo.com", 14, "GET", "/", NULL, NULL,
		    0, 1);
		return 0;
	case EXT_DDG:
		adv = ext_advance("ddg");
		if (adv == 0)
			return 0;
		if (adv > 0) {
			size_t blen = 0;
			const uint8_t *body = rump_tls_body(&blen);
			status = rump_tls_status_code();

			if (body != NULL && rump_tls_body_has(body, blen,
			    "duckduckgo"))
				printf("net: ext ddg ok (status=%d bytes=%lu "
				    "cookie=%d)\n", status,
				    (unsigned long)blen,
				    rump_tls_cookie()[0] != '\0');
			else
				printf("net: ext ddg FAIL (status=%d "
				    "bytes=%lu)\n", status,
				    (unsigned long)blen);
		}
		ext_start(EXT_DDG_STATUS, "duckduckgo.com", 14, "GET",
		    DUCKAI_STATUS, NULL, NULL, 0, 1);
		return 0;
	case EXT_DDG_STATUS:
		adv = ext_advance("duckai-status");
		if (adv == 0)
			return 0;
		if (adv > 0) {
			status = rump_tls_status_code();
			if (status == 200 && rump_tls_vqd()[0] != '\0')
				printf("net: ext duckai token ok (vqd=1 "
				    "cookie=%d)\n",
				    rump_tls_cookie()[0] != '\0');
			else
				printf("net: ext duckai token best-effort "
				    "(status=%d vqd=%d)\n", status,
				    rump_tls_vqd()[0] != '\0');
		}
		ext_start(EXT_DDG_CHAT, "duckduckgo.com", 14, "POST",
		    DUCKAI_PATH, "application/json", DUCKAI_BODY,
		    strlen(DUCKAI_BODY), 1);
		return 0;
	case EXT_DDG_CHAT:
		adv = ext_advance("duckai");
		if (adv == 0)
			return 0;
		if (adv > 0) {
			status = rump_tls_status_code();
			bytes = rump_tls_body_len();
			if (status == 200 && bytes > 0)
				printf("net: ext duckai ok (status=%d "
				    "bytes=%lu cookie=%d vqd=%d)\n", status,
				    (unsigned long)bytes,
				    rump_tls_cookie()[0] != '\0',
				    rump_tls_vqd()[0] != '\0');
			else
				printf("net: ext duckai best-effort (status=%d"
				    " bytes=%lu cookie=%d vqd=%d)\n", status,
				    (unsigned long)bytes,
				    rump_tls_cookie()[0] != '\0',
				    rump_tls_vqd()[0] != '\0');
		}
		printf("net: ext done\n");
		ext_state = EXT_DONE;
		return 1;
	}
	return 1;
}
