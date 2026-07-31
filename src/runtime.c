#ifdef _WIN32

#include <windows.h>
#include <stddef.h>

#ifdef _MSC_VER
#pragma comment(linker, "/alternatename:memcpy=flluf_memcpy")
#endif

void *flluf_memcpy(void *dest, const void *src, size_t n)
{
    char *d = dest;
    const char *s = src;
    while (n--)
        *d++ = *s++;
    return dest;
}

static void flluf_write(const char *buf, int len)
{
    HANDLE h = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD written;
    WriteFile(h, buf, (DWORD)len, &written, NULL);
}

static int flluf_write_cstr(const char *s)
{
    int len = 0;
    while (s[len])
        len++;
    flluf_write(s, len);
    return len;
}

static void flluf_reverse(char *buf, int len)
{
    for (int i = 0; i < len / 2; i++)
    {
        char t = buf[i];
        buf[i] = buf[len - 1 - i];
        buf[len - 1 - i] = t;
    }
}

static int flluf_itoa(long long v, char *out)
{
    int neg = v < 0;
    unsigned long long u = neg ? (unsigned long long)(-(v + 1)) + 1 : (unsigned long long)v;
    int i = 0;

    if (u == 0)
        out[i++] = '0';

    while (u > 0)
    {
        out[i++] = (char)('0' + (u % 10));
        u /= 10;
    }

    if (neg)
        out[i++] = '-';

    flluf_reverse(out, i);
    return i;
}

static int flluf_ftoa(double v, char *out)
{
    int neg = v < 0.0;
    double abs_v = neg ? -v : v;
    long long int_part = (long long)abs_v;
    double frac = abs_v - (double)int_part;
    unsigned long long frac_scaled = (unsigned long long)(frac * 1000000.0);
    char intbuf[32];
    char digits[6];
    int i = 0;
    int n;

    if (neg)
        out[i++] = '-';

    n = flluf_itoa(int_part, intbuf);
    for (int j = 0; j < n; j++)
        out[i++] = intbuf[j];

    out[i++] = '.';

    for (int j = 5; j >= 0; j--)
    {
        digits[j] = (char)('0' + (frac_scaled % 10));
        frac_scaled /= 10;
    }

    for (int j = 0; j < 6; j++)
        out[i++] = digits[j];

    return i;
}

void __flluf_log_i64(long long v)
{
    char buf[32];
    int n = flluf_itoa(v, buf);
    flluf_write(buf, n);
    flluf_write("\n", 1);
}

void __flluf_log_f64(double v)
{
    char buf[64];
    int n = flluf_ftoa(v, buf);
    flluf_write(buf, n);
    flluf_write("\n", 1);
}

void __flluf_log_ptr(const char *s)
{
    flluf_write_cstr(s);
    flluf_write("\n", 1);
}

void __flluf_exit(int code)
{
    ExitProcess((UINT)code);
}

#else

#include <stdio.h>
#include <stdlib.h>

void __flluf_log_i64(long long v)
{
    printf("%lld\n", v);
}

void __flluf_log_f64(double v)
{
    printf("%f\n", v);
}

void __flluf_log_ptr(const char *s)
{
    printf("%s\n", s);
}

void __flluf_exit(int code)
{
    exit(code);
}

#endif
