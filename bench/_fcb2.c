#include <stdio.h>
#include <stdlib.h>
long long fib(long long n){return n<2?n:fib(n-1)+fib(n-2);}
int main(int argc, char**argv){long long n = atoll(argv[1]); long long s=0; for(int i=0;i<5;i++)s+=fib(n);printf("%lld\n",s);}