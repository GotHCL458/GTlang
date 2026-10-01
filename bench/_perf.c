#include <stdio.h>
long long fib(long long n) { return n < 2 ? n : fib(n-1) + fib(n-2); }
long long sum(long long n) { long long s = 0; for (long long i = 0; i < n; i++) s += i; return s; }
int main() { printf("%lld\n%lld\n", fib(32), sum(20000000)); }