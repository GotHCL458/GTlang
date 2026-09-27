#include <stdio.h>
#include <string.h>
#include <time.h>
long long fib(long long n){ if(n<2) return n; return fib(n-1)+fib(n-2); }
long long loop_sum(void){ long long s=0; for(long long i=0;i<200000000;i++) s+=i; return s; }
int main(int argc,char**argv){
    clock_t t0=clock();
    long long r;
    if(argc>1&&strcmp(argv[1],"loop")==0) r=loop_sum(); else r=fib(35);
    clock_t t1=clock();
    printf("%lld  (%.1f ms)\n", r, (double)(t1-t0)*1000.0/CLOCKS_PER_SEC);
    return 0;
}
