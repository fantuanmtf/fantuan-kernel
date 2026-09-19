/* fantuan adaptation shim: sys/md5.h - MD5 for the RFC 1948 ISS hash
 * (ours, not upstream).  The adapter implements the algorithm in
 * rump_md5.c; the real NetBSD file carries an RSA notice and is not
 * imported.  Signatures and context layout mirror NetBSD's sys/md5.h. */
#ifndef FANTUAN_SYS_MD5_H
#define FANTUAN_SYS_MD5_H

#include <sys/types.h>

#define MD5_DIGEST_LENGTH	16
#define MD5_BLOCK_LENGTH	64

typedef struct MD5Context {
	uint32_t state[4];	/* state (ABCD) */
	uint32_t count[2];	/* number of bits, modulo 2^64 (lsb first) */
	unsigned char buffer[MD5_BLOCK_LENGTH];
} MD5_CTX;

void	MD5Init(MD5_CTX *);
void	MD5Update(MD5_CTX *, const unsigned char *, unsigned int);
void	MD5Final(unsigned char[MD5_DIGEST_LENGTH], MD5_CTX *);

#endif
