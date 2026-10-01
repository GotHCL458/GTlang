#include <stdio.h>
long long fib(long long n){return n<2?n:fib(n-1)+fib(n-2);}
int main(){long long s=0;for(int i=0;i<5;i++)s+=fib(30);printf("%lld\n",s);}