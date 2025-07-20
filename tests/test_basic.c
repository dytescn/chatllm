#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include <string.h>     // 解决 memcpy 警告
#include "ggml.h"

int main() {
    FILE *f = fopen("D:\\codes\\github\\dytescn\\chatllm\\models\\gpt2\\ggml.bin", "rb");
    if (!f) {
        perror("open failed");
        return 1;
    }

    char magic[4];
    fread(magic, 1, 4, f);
    printf("Magic: %.4s\n", magic);

    int32_t version;
    fread(&version, sizeof(int32_t), 1, f);
    printf("Version: %d\n", version);

    fclose(f);
    // return 0;

    struct ggml_init_params params = {
        .mem_size = 16 * 1024 * 1024,
        .mem_buffer = NULL,
        .no_alloc = false,
    };

    struct ggml_context *ctx = ggml_init(params);

    float data[4] = {1.0f, 2.0f, 3.0f, 4.0f};
    struct ggml_tensor *tensor = ggml_new_tensor_1d(ctx, GGML_TYPE_F32, 4);
    memcpy(tensor->data, data, sizeof(data));

    printf("Tensor data:\n");
    for (int i = 0; i < 4; i++) {
        printf("  %.2f\n", ((float *)tensor->data)[i]);
    }

    ggml_free(ctx);
    // return 0;
}
