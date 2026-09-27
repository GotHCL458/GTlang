#include <stdio.h>
#include <string.h>
long long fib(long long n) {
    if (n < 2) return n;
    return fib(n-1) + fib(n-2);
}
long long loop_sum(void) {
    long long s = 0;
    for (long long i = 0; i < 200000000; i++) s += i;
    return s;
}
int main(int argc, char** argv) {
    if (argc > 1 && strcmp(argv[1], "loop") == 0) printf("%lld\n", loop_sum());
    else printf("%lld\n", fib(35));
    return 0;
}
