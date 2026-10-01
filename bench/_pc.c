#include <stdio.h>
#include <stdlib.h>
long long fib(long long n){return n<2?n:fib(n-1)+fib(n-2);}
long long sum(long long n){long long s=0;for(long long i=0;i<n;i++)s+=i;return s;}
long long lst(long long n){long long*p=malloc(n*8);for(long long i=0;i<n;i++)p[i]=i;long long s=0;for(long long j=0;j<n;j++)s+=p[j];free(p);return s;}
int main(){printf("%lld\n%lld\n%lld\n",fib(32),sum(20000000),lst(2000000));}