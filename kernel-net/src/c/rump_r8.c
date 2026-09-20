/* rump_r8.c - R8 boot sequence: HTTPS over the pinned CA, the host UDP echo
 * test and the optional external phase (ours, M11 R8).  Runs after the R7
 * dns/ping/wget self-test, only with a DHCP lease.  The offline fixtures
 * (build-time FANTUAN_NET_FIXTURES=1) enable the HTTPS/UDP checks; the
 * external phase always tries the relay and skips when absent.  Nothing
 * here can fail the boot: failures print their marker and the sequence
 * moves on. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_shim.h"
#include "rump_udp_host.h"
#ifdef FANTUAN_TLS
#include "rump_tls.h"
#include "rump_ext.h"
#endif

#define R8_HTTPS_ADDR	0x0a000202u	/* 10.0.2.2 */
#define R8_HTTPS_PORT	18443
#define R8_HTTPS_HOST	"test.fantuan"
#define R8_LPORT	41943
#define R8_TIMEOUT	1000		/* PIT ticks (100 Hz): 10 s */

enum { R8_HTTPS, R8_UDP, R8_EXT, R8_DONE };
static int r8_state;

void
rump_r8_begin(void)
{

#ifdef FANTUAN_TLS
	if (!rump_tls_ready() || rump_tls_ca_len() == 0) {
		printf("tls: https skip (no ca)\n");
		r8_state = R8_UDP;
		return;
	}
	if (!rump_net_fixtures()) {
		printf("tls: https skip (fixtures off)\n");
		r8_state = R8_UDP;
		return;
	}
	rump_tls_request("GET", "/", NULL, NULL, 0, 0);
	rump_tls_begin(R8_HTTPS_HOST, strlen(R8_HTTPS_HOST), R8_HTTPS_ADDR,
	    R8_HTTPS_PORT, R8_LPORT, 1, R8_TIMEOUT);
	r8_state = R8_HTTPS;
#else
	r8_state = R8_UDP;
#endif
}

#ifdef FANTUAN_TLS
static int
r8_https_poll(void)
{
	int r = rump_tls_poll();

	if (r == 0)
		return 0;
	if (r < 0) {
		printf("tls: FAILED (%s)\n", rump_tls_error());
	} else {
		printf("net: https get ok (url=https://%s:%u/ bytes=%lu "
		    "hash=%08x)\n", R8_HTTPS_HOST, R8_HTTPS_PORT,
		    (unsigned long)rump_tls_body_len(),
		    rump_tls_body_hash());
	}
	return 1;
}
#endif

int
rump_r8_poll(void)
{

	switch (r8_state) {
	case R8_HTTPS:
#ifdef FANTUAN_TLS
		if (!r8_https_poll())
			return 0;
#endif
		r8_state = R8_UDP;
		return 0;
	case R8_UDP:
		if (!rump_net_fixtures()) {
			printf("net: udp host skip (fixtures off)\n");
#ifdef FANTUAN_TLS
			rump_ext_begin();
#endif
			r8_state = R8_EXT;
			return 0;
		}
		if (rump_udp_host_run() != 0) {
			printf("net: udp host FAILED (tx=%d rx=%d)\n",
			    rump_udp_host_tx(), rump_udp_host_rx());
		} else {
			printf("net: udp host ok (tx=%d rx=%d bytes=%lu)\n",
			    rump_udp_host_tx(), rump_udp_host_rx(),
			    (unsigned long)rump_udp_host_bytes());
		}
#ifdef FANTUAN_TLS
		rump_ext_begin();
#endif
		r8_state = R8_EXT;
		return 0;
	case R8_EXT:
#ifdef FANTUAN_TLS
		if (rump_ext_poll() == 0)
			return 0;
#endif
		r8_state = R8_DONE;
		return 1;
	default:
		return 1;
	}
}
