/* Hand-written header for cb-cdylib-c-abi. Keep in sync with src/lib.rs
 * (cbindgen can generate this from the Rust source instead). */
#ifndef CB_C_ABI_H
#define CB_C_ABI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Bump CB_ABI_VERSION on any breaking change; callers can compare at startup. */
#define CB_ABI_VERSION 1u
uint32_t cb_abi_version(void);

/* Error codes: 0 = success, negative = failure. Details: cb_last_error_message. */
enum {
    CB_OK = 0,
    CB_ERR_NULL = -1,
    CB_ERR_OVERFLOW = -2,
    CB_ERR_BUFFER_TOO_SMALL = -3,
    CB_ERR_INVALID_UTF8 = -4,
    CB_ERR_PANIC = -5
};

/* Opaque handle: callers only hold pointers; layout is private to Rust. */
typedef struct cb_counter cb_counter;
cb_counter *cb_counter_new(int64_t start);              /* free with cb_counter_free */
void cb_counter_free(cb_counter *c);                    /* NULL is allowed */
int cb_counter_add(cb_counter *c, int64_t delta);
int cb_counter_get(const cb_counter *c, int64_t *out);

/* Callback with user_data: Rust never inspects the void*, only passes it back.
 * The callback must not longjmp/throw across the boundary. */
typedef void (*cb_visit_fn)(int64_t value, void *user_data);
int cb_counter_history_each(const cb_counter *c, cb_visit_fn f, void *user_data);

/* Strings. Rust -> C: returned char* is OWNED by the caller and must be released
 * with cb_string_free (never free()!, the allocators differ). C -> Rust: input
 * const char* stays owned by the caller and is only borrowed for the call. */
char *cb_greet(const char *name);
void cb_string_free(char *s);

/* Caller-owned buffer: on CB_ERR_BUFFER_TOO_SMALL *written is the size needed. */
int cb_fill_buffer(uint8_t *buf, size_t cap, size_t *written);

/* Thread-local message of the last failure on this thread; NULL if none.
 * Valid only until the next cb_* call on the same thread. Do not free. */
const char *cb_last_error_message(void);

/* Always panics inside Rust; shows that panics become CB_ERR_PANIC. */
int cb_test_panic(void);

/* Versioned struct: first field is the size the caller compiled against, so
 * newer libraries can append fields without breaking old callers. */
typedef struct {
    uint32_t struct_size;
    int32_t x;
    int32_t y;
} cb_point;
_Static_assert(sizeof(cb_point) == 12, "cb_point layout changed");
int cb_point_init(cb_point *p, int32_t x, int32_t y);

#ifdef __cplusplus
}
#endif
#endif
