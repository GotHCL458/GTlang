#include <stdio.h>
#include <string.h>
static long long g_state = 1;
static long long step(void) { g_state = (g_state * 1103515245 + 12345) & 0x7fffffff; return g_state; }
long long loop_lcg(void) { long long s = 0; for (long long i = 0; i < 10000000; i++) s = (s + step()) % 1000000007; return s; }
static long long a[1000];
long long arr_sum(void) { for (int i = 0; i < 1000; i++) a[i] = i * 997; long long s = 0; for (long long r = 0; r < 100000; r++) for (int i = 0; i < 1000; i++) s += a[i]; return s; }
long long fib(long long n) { if (n < 2) return n; return fib(n-1) + fib(n-2); }
long long loop_sum(void) { long long s = 0; for (long long i = 0; i < 200000000; i++) s += i; return s; }
int main(int argc, char** argv) {
    if (argc > 1 && strcmp(argv[1], "lcg") == 0) printf("%lld\n", loop_lcg());
    else if (argc > 1 && strcmp(argv[1], "arr") == 0) printf("%lld\n", arr_sum());
    else if (argc > 1 && strcmp(argv[1], "fib") == 0) printf("%lld\n", fib(35));
    else printf("%lld\n", loop_sum());
    return 0;
}
