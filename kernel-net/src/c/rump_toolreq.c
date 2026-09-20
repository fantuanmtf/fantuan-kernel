/* rump_toolreq.c - shell tool request slot (ours, M11 R7).
 * The shell must not call into the stack itself: the adapter's locks assume
 * the single net task owns socket processing, and a second task that takes
 * a socket lock mid-operation can deadlock the net task on this pre-SMP
 * shim.  So a shell command only fills the request slot; loopback_task's
 * rump_net_poll() steps the client state machine and publishes the result,
 * and the shell waits on rump_tool_status(). */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_shim.h"
#ifdef FANTUAN_TLS
#include "rump_tls.h"
#endif

enum {
	TQ_IDLE, TQ_DNS, TQ_PING, TQ_WGET, TQ_WGET_TLS, TQ_DONE, TQ_FAILED
};

static int tq_kind, tq_state;
static const char *tq_error;
static uint32_t tq_addr, tq_hash;
static int tq_rtt, tq_status;
static size_t tq_bytes;

static void
tq_begin(int kind)
{

	tq_kind = kind;
	tq_state = 1;
	tq_error = NULL;
	tq_addr = tq_hash = 0;
	tq_rtt = tq_status = 0;
	tq_bytes = 0;
}

void
rump_tool_begin_dns(const char *name, size_t len, uint32_t server,
    uint16_t port)
{

	tq_begin(TQ_DNS);
	rump_dns_start(name, len, server, port);
}

void
rump_tool_begin_ping(uint32_t addr)
{

	tq_begin(TQ_PING);
	rump_ping_begin_addr(addr);
}

void
rump_tool_begin_wget(const char *host, size_t hostlen, uint32_t addr,
    uint16_t port, uint16_t lport)
{

	tq_begin(TQ_WGET);
	rump_http_begin_wget(host, hostlen, addr, port, lport, 0);
}

#ifdef FANTUAN_TLS
void
rump_tool_begin_wget_tls(const char *host, size_t hostlen, uint32_t addr,
    uint16_t port, uint16_t lport, int verify)
{

	tq_begin(TQ_WGET_TLS);
	rump_tls_request("GET", "/", NULL, NULL, 0, 0);
	rump_tls_begin(host, hostlen, addr, port, lport, verify, 2000);
}
#endif

/* Called from rump_net_poll() in the net task; a no-op without a request. */
int
rump_tool_poll(void)
{
	int r;

	if (tq_state != 1)
		return 0;
	switch (tq_kind) {
	case TQ_DNS:
		r = rump_dns_poll();
		if (r < 0) {
			tq_error = rump_dns_error();
			tq_state = TQ_FAILED;
		} else if (r > 0) {
			tq_addr = rump_dns_result();
			tq_state = TQ_DONE;
		}
		break;
	case TQ_PING:
		r = rump_ping_poll();
		if (r < 0) {
			tq_error = rump_ping_error();
			tq_state = TQ_FAILED;
		} else if (r > 0) {
			tq_rtt = rump_ping_rtt();
			tq_state = TQ_DONE;
		}
		break;
	case TQ_WGET:
		r = rump_http_poll();
		if (r < 0) {
			tq_error = rump_http_error();
			tq_state = TQ_FAILED;
		} else if (r > 0) {
			tq_status = rump_http_status_code();
			tq_bytes = rump_http_body_len();
			tq_hash = rump_http_body_hash();
			tq_state = TQ_DONE;
		}
		break;
#ifdef FANTUAN_TLS
	case TQ_WGET_TLS:
		r = rump_tls_poll();
		if (r < 0) {
			tq_error = rump_tls_error();
			tq_state = TQ_FAILED;
		} else if (r > 0) {
			tq_status = rump_tls_status_code();
			tq_bytes = rump_tls_body_len();
			tq_hash = rump_tls_body_hash();
			tq_state = TQ_DONE;
		}
		break;
#endif
	default:
		tq_state = TQ_FAILED;
		break;
	}
	return 0;
}

int
rump_tool_status(void)
{

	if (tq_state == TQ_DONE)
		return 1;
	if (tq_state == TQ_FAILED)
		return -1;
	return 0;
}

const char *
rump_tool_error(void)
{

	return tq_error != NULL ? tq_error : "unknown";
}

uint32_t
rump_tool_result_addr(void)
{

	return tq_addr;
}

int
rump_tool_result_rtt(void)
{

	return tq_rtt;
}

int
rump_tool_result_http(void)
{

	return tq_status;
}

size_t
rump_tool_result_bytes(void)
{

	return tq_bytes;
}

uint32_t
rump_tool_result_hash(void)
{

	return tq_hash;
}
