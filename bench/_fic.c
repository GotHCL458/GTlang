#include <stdio.h>
long long fib(long long n){long long a=0,b=1,i=0;while(i<n){long long t=a+b;a=b;b=t;i++;}return a;}
int main(){long long s=0;for(int i=0;i<5;i++)s+=fib(1000000);printf("%lld\n",s);}