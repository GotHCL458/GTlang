#include "GTO/gto.h"
#include <stdio.h>
static GtoChan *ch;
static void producer(void *arg) {
    for (int64_t i = 0; i < 10; i++) gto_chan_send(ch, i * i);
    gto_chan_close(ch);
}
static void consumer(void *arg) {
    int64_t v;
    int64_t sum = 0;
    while ((v = gto_chan_recv(ch)) >= 0) sum += v;
    printf("sum=%ld\n", (long)sum);
}
static void pool_task(void *arg) { printf("task %ld\n", (long)(intptr_t)arg); }
int main(void) {
    gto_init(4);
    ch = gto_chan_new(0);
    gto_spawn(producer, NULL);
    gto_spawn(consumer, NULL);
    gto_wait_all();
    gto_chan_free(ch);
    GtoPool *p = gto_pool_new(2);
    for (int i = 0; i < 4; i++) gto_pool_submit(p, pool_task, (void *)(intptr_t)i);
    gto_pool_wait(p);
    gto_pool_free(p);
    gto_shutdown();
    printf("GTO %s ok\n", gto_version());
    return 0;
}
