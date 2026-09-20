/* rump_tls_int.h - internal state shared by the TLS client files (ours).
 * The public interface is rump_tls.h; this header is only for the split
 * between rump_tls.c (session driver), rump_tls_io.c (BIO + HTTP over TLS)
 * and rump_tls_conf.c (config/init + request spec). */
#ifndef FANTUAN_RUMP_TLS_INT_H
#define FANTUAN_RUMP_TLS_INT_H

#include <sys/types.h>
#include <sys/socket.h>
#include "mbedtls/ctr_drbg.h"
#include "mbedtls/entropy.h"
#include "mbedtls/ssl.h"
#include "mbedtls/x509_crt.h"

#define TLS_RX_MAX	16384
#define TLS_HOST_MAX	96
#define TLS_PATH_MAX	128
#define TLS_BODY_MAX	192
#define TLS_JAR_MAX	256
#define TLS_VQD_MAX	96
#define TLS_TX_MAX	(TLS_PATH_MAX + TLS_HOST_MAX + TLS_BODY_MAX + \
			 TLS_JAR_MAX + TLS_VQD_MAX + 256)

enum {
	TLS_IDLE, TLS_CONNECT, TLS_HANDSHAKE, TLS_SEND, TLS_RECV,
	TLS_DONE, TLS_FAILED
};

/* Config and request spec (rump_tls_conf.c). */
extern mbedtls_ssl_config tls_conf;
extern mbedtls_x509_crt tls_ca;
extern int tls_ca_ok, tls_inited;
extern char tls_method[6], tls_path[TLS_PATH_MAX], tls_ctype[48];
extern char tls_body[TLS_BODY_MAX], tls_cookie[TLS_JAR_MAX];
extern char tls_vqd[TLS_VQD_MAX];
extern size_t tls_bodylen;
extern int tls_capture;

/* Session state (rump_tls.c). */
extern struct socket *tls_so;
extern mbedtls_ssl_context tls_ssl;
extern int tls_ssl_up, tls_phase, tls_verify;
extern uint64_t tls_t0;
extern uint32_t tls_timeout;
extern const char *tls_step;
extern char tls_host[TLS_HOST_MAX];
extern uint8_t tls_tx[TLS_TX_MAX], tls_rx[TLS_RX_MAX];
extern size_t tls_tx_len, tls_tx_off, tls_rx_len, tls_body_off;
extern int tls_status, tls_truncated;
extern size_t tls_bytes;
extern uint32_t tls_hash;

int tls_fail(const char *step);
void tls_close(void);
int tls_handshake_start(void);
int tls_build_request(void);
int tls_recv_step(void);
int tls_report(void);

#endif
