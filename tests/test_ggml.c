#include <stdio.h>
#include "ggml.h"

void callback(enum ggml_log_level level, const char *text, void *user_data) {
    (void)level;
    (void)user_data;
    printf("[ggml] %s", text);
}

int main() {
    ggml_log_set(callback, NULL);
    return 0;
}