/*
 * fantuan mbedTLS configuration for kernel-net (ours, M11 R8).
 * Selected with -DMBEDTLS_CONFIG_FILE='"fantuan_mbedtls_config.h"'.
 *
 * Kernel constraints: no libc, no filesystem, no threads, no wall clock and
 * no network syscalls.  The transport is the rump socket layer (rump_tls.c),
 * memory comes from the adapter kmem arena, time from the PIT and entropy
 * from the boot-seeded CSPRNG (rump_tls_entropy.c).  TLS 1.2 client only.
 */
#ifndef FANTUAN_MBEDTLS_CONFIG_H
#define FANTUAN_MBEDTLS_CONFIG_H

#define MBEDTLS_CONFIG_VERSION 0x03000000

/* Platform: custom allocators, PIT time, no std C functions we cannot back. */
#define MBEDTLS_PLATFORM_C
#define MBEDTLS_PLATFORM_MEMORY
#define MBEDTLS_HAVE_TIME
#define MBEDTLS_PLATFORM_TIME_ALT
#define MBEDTLS_PLATFORM_MS_TIME_ALT
#define MBEDTLS_PLATFORM_NO_STD_FUNCTIONS
#define MBEDTLS_PLATFORM_SNPRINTF_ALT
#define MBEDTLS_PLATFORM_VSNPRINTF_ALT

/* Entropy: no platform source; the hardware poll is our boot CSPRNG. */
#define MBEDTLS_NO_PLATFORM_ENTROPY
#define MBEDTLS_ENTROPY_HARDWARE_ALT
#define MBEDTLS_ENTROPY_C
#define MBEDTLS_ENTROPY_FORCE_SHA256
#define MBEDTLS_CTR_DRBG_C

/* Symmetric crypto: AES-GCM only. */
#define MBEDTLS_AES_C
#define MBEDTLS_GCM_C
#define MBEDTLS_CIPHER_C
#define MBEDTLS_AES_ROM_TABLES

/* Hashes and key derivation material. */
#define MBEDTLS_SHA256_C
#define MBEDTLS_MD_C

/* Public key / X.509: RSA (PKCS#1 v1.5 and PSS), DER parsing only. */
#define MBEDTLS_BIGNUM_C
#define MBEDTLS_RSA_C
#define MBEDTLS_PKCS1_V15
#define MBEDTLS_PKCS1_V21
#define MBEDTLS_PK_C
#define MBEDTLS_PK_PARSE_C
#define MBEDTLS_OID_C
#define MBEDTLS_ASN1_PARSE_C
#define MBEDTLS_X509_USE_C
#define MBEDTLS_X509_CRT_PARSE_C

/* Key exchange: ECDHE-RSA with secp256r1. */
#define MBEDTLS_ECP_C
#define MBEDTLS_ECP_DP_SECP256R1_ENABLED
#define MBEDTLS_ECDH_C
#define MBEDTLS_KEY_EXCHANGE_ECDHE_RSA_ENABLED

/* TLS: client, TLS 1.2, one GCM suite, SNI, bounded records. */
#define MBEDTLS_SSL_TLS_C
#define MBEDTLS_SSL_CLI_C
#define MBEDTLS_SSL_PROTO_TLS1_2
#define MBEDTLS_SSL_SERVER_NAME_INDICATION
#define MBEDTLS_SSL_KEEP_PEER_CERTIFICATE
#define MBEDTLS_SSL_CIPHERSUITES MBEDTLS_TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
#define MBEDTLS_SSL_IN_CONTENT_LEN 16384
#define MBEDTLS_SSL_OUT_CONTENT_LEN 4096
#define MBEDTLS_SSL_MAX_FRAGMENT_LENGTH

/* Explicitly absent: FS_IO, NET_C, TIMING_C, THREADING_C, PEM/BASE64,
 * TLS1_3, DTLS, session tickets, renegotiation, PSA crypto, self-tests. */

#endif /* FANTUAN_MBEDTLS_CONFIG_H */
