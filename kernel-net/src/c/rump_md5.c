/* rump_md5.c - MD5 for TCP's RFC 1948 ISS hash (ours).
 * The real NetBSD sys/md5.h carries an RSA Data Security notice and is not
 * part of the import, so the adapter ships its own implementation of the
 * RFC 1321 algorithm.  tcp_do_rfc1948 defaults to 0; the hash path is only
 * entered when a sysctl enables it, so this stays cold. */
#include <sys/types.h>
#include <sys/md5.h>
#include <sys/systm.h>

static const uint32_t md5_t[64] = {
	0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee,
	0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
	0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
	0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
	0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,
	0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
	0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
	0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
	0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
	0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
	0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05,
	0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
	0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039,
	0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
	0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
	0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
};

#define MD5_F(x, y, z)	(((z) ^ ((x) & ((y) ^ (z)))))
#define MD5_G(x, y, z)	(((y) ^ ((z) & ((x) ^ (y)))))
#define MD5_H(x, y, z)	((x) ^ (y) ^ (z))
#define MD5_I(x, y, z)	((y) ^ ((x) | ~(z)))
#define MD5_ROTL(x, n)	(((x) << (n)) | ((x) >> (32 - (n))))

#define MD5_STEP(f, a, b, c, d, x, t, s)		\
	do {						\
		(a) += f((b), (c), (d)) + (x) + (t);	\
		(a) = MD5_ROTL((a), (s)) + (b);		\
	} while (0)

static void
md5_transform(uint32_t state[4], const unsigned char block[MD5_BLOCK_LENGTH])
{
	uint32_t x[16], a, b, c, d;
	int i;

	for (i = 0; i < 16; i++) {
		x[i] = (uint32_t)block[i * 4] |
		    ((uint32_t)block[i * 4 + 1] << 8) |
		    ((uint32_t)block[i * 4 + 2] << 16) |
		    ((uint32_t)block[i * 4 + 3] << 24);
	}

	a = state[0];
	b = state[1];
	c = state[2];
	d = state[3];

	for (i = 0; i < 16; i += 4) {
		MD5_STEP(MD5_F, a, b, c, d, x[i], md5_t[i], 7);
		MD5_STEP(MD5_F, d, a, b, c, x[i + 1], md5_t[i + 1], 12);
		MD5_STEP(MD5_F, c, d, a, b, x[i + 2], md5_t[i + 2], 17);
		MD5_STEP(MD5_F, b, c, d, a, x[i + 3], md5_t[i + 3], 22);
	}
	for (i = 0; i < 16; i += 4) {
		MD5_STEP(MD5_G, a, b, c, d, x[(i + 1) & 15], md5_t[16 + i], 5);
		MD5_STEP(MD5_G, d, a, b, c, x[(i + 6) & 15], md5_t[17 + i], 9);
		MD5_STEP(MD5_G, c, d, a, b, x[(i + 11) & 15], md5_t[18 + i], 14);
		MD5_STEP(MD5_G, b, c, d, a, x[i & 15], md5_t[19 + i], 20);
	}
	for (i = 0; i < 16; i += 4) {
		MD5_STEP(MD5_H, a, b, c, d, x[(3 * i + 5) & 15], md5_t[32 + i], 4);
		MD5_STEP(MD5_H, d, a, b, c, x[(3 * i + 8) & 15], md5_t[33 + i], 11);
		MD5_STEP(MD5_H, c, d, a, b, x[(3 * i + 11) & 15], md5_t[34 + i], 16);
		MD5_STEP(MD5_H, b, c, d, a, x[(3 * i + 14) & 15], md5_t[35 + i], 23);
	}
	for (i = 0; i < 16; i += 4) {
		MD5_STEP(MD5_I, a, b, c, d, x[(7 * i) & 15], md5_t[48 + i], 6);
		MD5_STEP(MD5_I, d, a, b, c, x[(7 * i + 7) & 15], md5_t[49 + i], 10);
		MD5_STEP(MD5_I, c, d, a, b, x[(7 * i + 14) & 15], md5_t[50 + i], 15);
		MD5_STEP(MD5_I, b, c, d, a, x[(7 * i + 21) & 15], md5_t[51 + i], 21);
	}

	state[0] += a;
	state[1] += b;
	state[2] += c;
	state[3] += d;
}

void
MD5Init(MD5_CTX *ctx)
{

	ctx->state[0] = 0x67452301;
	ctx->state[1] = 0xefcdab89;
	ctx->state[2] = 0x98badcfe;
	ctx->state[3] = 0x10325476;
	ctx->count[0] = 0;
	ctx->count[1] = 0;
}

void
MD5Update(MD5_CTX *ctx, const unsigned char *data, unsigned int len)
{
	unsigned int have, need, offset = 0;
	uint32_t bits = (uint32_t)len << 3;

	have = (ctx->count[0] >> 3) & (MD5_BLOCK_LENGTH - 1);
	ctx->count[0] += bits;
	if (ctx->count[0] < bits)
		ctx->count[1]++;
	ctx->count[1] += (uint32_t)len >> 29;

	need = MD5_BLOCK_LENGTH - have;
	if (len >= need) {
		memcpy(ctx->buffer + have, data, need);
		md5_transform(ctx->state, ctx->buffer);
		offset = need;
		while (offset + MD5_BLOCK_LENGTH <= len) {
			md5_transform(ctx->state, data + offset);
			offset += MD5_BLOCK_LENGTH;
		}
		have = 0;
	}
	memcpy(ctx->buffer + have, data + offset, len - offset);
}

void
MD5Final(unsigned char digest[MD5_DIGEST_LENGTH], MD5_CTX *ctx)
{
	static const unsigned char pad[MD5_BLOCK_LENGTH] = { 0x80 };
	unsigned char lenbuf[8];
	unsigned int have;
	int i;

	have = (ctx->count[0] >> 3) & (MD5_BLOCK_LENGTH - 1);
	MD5Update(ctx, pad, (have < 56) ? (56 - have) : (120 - have));

	for (i = 0; i < 4; i++) {
		lenbuf[i] = (unsigned char)(ctx->count[0] >> (8 * i));
		lenbuf[4 + i] = (unsigned char)(ctx->count[1] >> (8 * i));
	}
	MD5Update(ctx, lenbuf, 8);

	for (i = 0; i < 4; i++) {
		digest[i * 4] = (unsigned char)ctx->state[i];
		digest[i * 4 + 1] = (unsigned char)(ctx->state[i] >> 8);
		digest[i * 4 + 2] = (unsigned char)(ctx->state[i] >> 16);
		digest[i * 4 + 3] = (unsigned char)(ctx->state[i] >> 24);
	}
}
