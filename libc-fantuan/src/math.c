/* libc-fantuan — math (P1): fabs/copysign/trunc are exact; floor/ceil/round
 * are real for the finite range configure uses; the transcendental stubs
 * return 0 so configure probes link (P2 fills them in if needed). */
#include <math.h>

double fabs(double x)
{
    union {
        double d;
        unsigned long u;
    } v = { x };
    v.u &= 0x7fffffffffffffffUL;
    return v.d;
}

float fabsf(float x)
{
    union {
        float f;
        unsigned int u;
    } v = { x };
    v.u &= 0x7fffffffU;
    return v.f;
}

long double fabsl(long double x)
{
    return x < 0 ? -x : x;
}

double trunc(double x)
{
    return (double)(long long)x;
}

double floor(double x)
{
    long long i = (long long)x;
    if ((double)i > x) {
        i--;
    }
    return (double)i;
}

double ceil(double x)
{
    long long i = (long long)x;
    if ((double)i < x) {
        i++;
    }
    return (double)i;
}

double round(double x)
{
    return x >= 0 ? floor(x + 0.5) : ceil(x - 0.5);
}

double copysign(double x, double y)
{
    union {
        double d;
        unsigned long u;
    } a = { x }, b = { y };
    a.u = (a.u & 0x7fffffffffffffffUL) | (b.u & 0x8000000000000000UL);
    return a.d;
}

double fmod(double x, double y)
{
    if (y == 0.0) {
        return NAN;
    }
    double q = trunc(x / y);
    return x - q * y;
}

int __isnan(double x) { return x != x; }
int __isinf(double x) { return x > 0 && x * 2 == x ? 1 : (x < 0 && x * 2 == x ? -1 : 0); }

/* Stubs: link-compatible, honestly not implemented in P1. */
double sqrt(double x) { (void)x; return 0.0; }
double pow(double x, double y) { (void)x; (void)y; return 0.0; }
double exp(double x) { (void)x; return 0.0; }
double log(double x) { (void)x; return 0.0; }
double log10(double x) { (void)x; return 0.0; }
double sin(double x) { (void)x; return 0.0; }
double cos(double x) { (void)x; return 0.0; }
double tan(double x) { (void)x; return 0.0; }
double atan(double x) { (void)x; return 0.0; }
double atan2(double y, double x) { (void)y; (void)x; return 0.0; }
double frexp(double x, int *exp) { (void)x; if (exp) *exp = 0; return 0.0; }
double ldexp(double x, int exp) { (void)exp; return x; }
double modf(double x, double *iptr) { if (iptr) *iptr = trunc(x); return x - trunc(x); }
