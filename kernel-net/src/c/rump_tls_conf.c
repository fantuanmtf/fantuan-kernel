/* rump_tls_conf.c - mbedTLS config, init and request spec (ours, M11 R8).
 * Part of the split TLS client: this file owns the one-time platform/config
 * setup (allocators/time/CSPRNG/CA/KATs live in rump_tls_entropy.c and
 * rump_tls_kat.c) and the request spec the session driver sends. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_shim.h"
#include "rump_tls.h"
#include "rump_tls_int.h"

#include <string.h>

static mbedtls_entropy_context tls_entropy;
static mbedtls_ctr_drbg_context tls_drbg;

mbedtls_ssl_config tls_conf;
mbedtls_x509_crt tls_ca;
int tls_ca_ok;
int tls_inited;

char tls_method[6], tls_path[TLS_PATH_MAX], tls_ctype[48];
char tls_body[TLS_BODY_MAX], tls_cookie[TLS_JAR_MAX];
char tls_vqd[TLS_VQD_MAX];
size_t tls_bodylen;
int tls_capture;

extern int rump_tls_platform_init(void);
extern void rump_tls_seed(void);
extern const uint8_t *rump_tls_ca_der(size_t *len);

int
rump_tls_ready(void)
{

	return tls_inited;
}

size_t
rump_tls_ca_len(void)
{
	size_t len = 0;

	(void)rump_tls_ca_der(&len);
	return len;
}

int
rump_tls_init(void)
{
	const uint8_t *ca;
	size_t calen = 0;
	static const int ciphersuites[] = {
		MBEDTLS_TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256, 0
	};

	if (tls_inited)
		return 0;
	(void)rump_tls_platform_init();
	rump_tls_seed();
	mbedtls_entropy_init(&tls_entropy);
	mbedtls_ctr_drbg_init(&tls_drbg);
	mbedtls_x509_crt_init(&tls_ca);
	mbedtls_ssl_config_init(&tls_conf);
	if (mbedtls_ctr_drbg_seed(&tls_drbg, mbedtls_entropy_func,
	    &tls_entropy, (const unsigned char *)"fantuan-tls", 11) != 0) {
		printf("tls: FAILED (rng)\n");
		return -1;
	}
	ca = rump_tls_ca_der(&calen);
	if (calen > 0 && mbedtls_x509_crt_parse_der(&tls_ca, ca, calen) == 0)
		tls_ca_ok = 1;
	if (mbedtls_ssl_config_defaults(&tls_conf, MBEDTLS_SSL_IS_CLIENT,
	    MBEDTLS_SSL_TRANSPORT_STREAM, MBEDTLS_SSL_PRESET_DEFAULT) != 0) {
		printf("tls: FAILED (config)\n");
		return -1;
	}
	mbedtls_ssl_conf_rng(&tls_conf, mbedtls_ctr_drbg_random, &tls_drbg);
	mbedtls_ssl_conf_ciphersuites(&tls_conf, ciphersuites);
	tls_inited = 1;
	return 0;
}

static size_t
tls_copy(char *dst, size_t cap, const char *src)
{
	size_t n;

	if (src == NULL) {
		dst[0] = '\0';
		return 0;
	}
	n = strlen(src);
	if (n >= cap)
		n = cap - 1;
	memcpy(dst, src, n);
	dst[n] = '\0';
	return n;
}

void
rump_tls_request(const char *method, const char *path, const char *ctype,
    const char *body, size_t bodylen, int capture)
{

	(void)tls_copy(tls_method, sizeof(tls_method), method);
	(void)tls_copy(tls_path, sizeof(tls_path), path);
	(void)tls_copy(tls_ctype, sizeof(tls_ctype), ctype);
	tls_bodylen = 0;
	tls_body[0] = '\0';
	if (body != NULL && bodylen > 0) {
		if (bodylen >= sizeof(tls_body))
			bodylen = sizeof(tls_body) - 1;
		memcpy(tls_body, body, bodylen);
		tls_body[bodylen] = '\0';
		tls_bodylen = bodylen;
	}
	tls_capture = capture;
}

void
rump_tls_set_cookie(const char *cookie)
{

	(void)tls_copy(tls_cookie, sizeof(tls_cookie), cookie);
}

void
rump_tls_set_vqd(const char *vqd)
{

	(void)tls_copy(tls_vqd, sizeof(tls_vqd), vqd);
}
