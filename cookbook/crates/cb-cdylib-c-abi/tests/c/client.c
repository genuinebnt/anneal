/* C client: exercises the library only through the public header. */
#include <stdio.h>
#include <string.h>
#include "cb_c_abi.h"

static void collect(int64_t v, void *ud) {
    int64_t *sum = ud;       /* user_data carries the caller's state */
    *sum += v;
}

int main(void) {
    if (cb_abi_version() != CB_ABI_VERSION) { puts("abi mismatch"); return 10; }

    cb_counter *c = cb_counter_new(10);
    cb_counter_add(c, 5);
    cb_counter_add(c, 7);
    int64_t v = 0, sum = 0;
    cb_counter_get(c, &v);
    cb_counter_history_each(c, collect, &sum);
    printf("value=%lld history_sum=%lld\n", (long long)v, (long long)sum);

    /* error code + message */
    int rc = cb_counter_add(NULL, 1);
    printf("null add rc=%d msg=%s\n", rc, cb_last_error_message());
    cb_counter_free(c);

    char *g = cb_greet("C");
    printf("%s\n", g);
    cb_string_free(g);          /* NOT free(g) */

    uint8_t small[4], big[64];
    size_t need = 0;
    rc = cb_fill_buffer(small, sizeof small, &need);
    printf("small rc=%d need=%zu\n", rc, need);
    rc = cb_fill_buffer(big, sizeof big, &need);
    printf("big rc=%d text=%.*s\n", rc, (int)need, (char *)big);

    printf("panic rc=%d msg=%s\n", cb_test_panic(), cb_last_error_message());

    cb_point p;
    cb_point_init(&p, 3, 4);
    printf("point size=%u x=%d y=%d\n", p.struct_size, p.x, p.y);

    return (v == 22 && sum == 10 + 15 + 22) ? 0 : 1;   /* exit code is asserted */
}
