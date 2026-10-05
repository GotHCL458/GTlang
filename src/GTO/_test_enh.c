#include "GTO/gto.h"
#include <stdio.h>
static GtoSem *sem;
static GtoBarrier *bar;
static int64_t once_flag = 0;
static void once_fn(void) { printf("once!\n"); }
static void worker(void *arg) {
    int64_t id = (int64_t)(intptr_t)arg;
    gto_tls_set(0, (void *)(intptr_t)(id * 100));
    printf("w%ld tls=%ld\n", (long)id, (long)(intptr_t)gto_tls_get(0));
    gto_barrier_wait(bar);
    gto_sem_post(sem);
    gto_once(&once_flag, once_fn);
}
int main(void) {
    gto_init(4);
    sem = gto_sem_new(0);
    bar = gto_barrier_new(3);
    for (int i = 0; i < 3; i++) gto_spawn(worker, (void *)(intptr_t)i);
    for (int i = 0; i < 3; i++) gto_sem_wait(sem);
    gto_wait_all();
    GtoStats st; gto_stats(&st);
    printf("spawned=%ld done=%ld live=%ld workers=%d\n", (long)st.coros_spawned, (long)st.coros_done, (long)st.live_coros, st.n_workers);
    gto_barrier_free(bar); gto_sem_free(sem);
    gto_shutdown();
    printf("GTO enhanced ok\n");
    return 0;
}
