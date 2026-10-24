// Checks that mq.h compiles as C++ and that the API links and works from it.
#include <cstdio>
#include <cstring>
#include "mq.h"

#define CHECK(cond)                                              \
    do {                                                         \
        if (!(cond)) {                                           \
            std::fprintf(stderr, "FAIL: %s (line %d)\n", #cond, __LINE__); \
            return 1;                                            \
        }                                                        \
    } while (0)

int main() {
    mq_context_t *ctx = NULL;
    mq_error_t *err = NULL;
    CHECK(mq_engine_new(&ctx, &err) == MQ_OK);

    mq_values_t *values = NULL;
    CHECK(mq_eval_ex(ctx, ".h", "# Title", "markdown", &values, &err) == MQ_OK);
    CHECK(mq_values_len(values) == 1);
    CHECK(mq_value_type(values, 0) == MQ_TYPE_MARKDOWN);
    CHECK(std::strcmp(mq_value_node_kind(values, 0), "h1") == 0);
    mq_values_free(values);

    CHECK(mq_eval_ex(ctx, "|", "x", "text", &values, &err) == MQ_ERR_SYNTAX);
    mq_location_t at;
    CHECK(mq_error_location(err, &at));
    mq_error_free(err);

    mq_result_t legacy = mq_eval(ctx, ".h", "# Title", "markdown");
    CHECK(legacy.error_msg == NULL && legacy.values_len == 1);
    mq_free_result(legacy);

    mq_destroy(ctx);
    std::printf("C++ header test passed\n");
    return 0;
}
