#include "ggml.h"
#include "ggml-cpu.h"
#include "ggml-alloc.h"
#include "ggml-backend.h"

#ifdef GGML_USE_CUDA
#include "ggml-cuda.h"
#endif

#ifdef GGML_USE_METAL
#include "ggml-metal.h"
#endif

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// 默认日志回调
static void ggml_log_callback_default(enum ggml_log_level level, const char *text, void *user_data) {
    (void) level;
    (void) user_data;
    fputs(text, stderr);
    fflush(stderr);
}

struct simple_model {
    struct ggml_tensor *a;
    struct ggml_tensor *b;
    ggml_backend_t backend;
    ggml_backend_buffer_t buffer;
    struct ggml_context *ctx;
};

// 初始化模型
void load_model(struct simple_model *model, float *a, float *b, int rows_A, int cols_A, int rows_B, int cols_B) {
    ggml_log_set(ggml_log_callback_default, NULL);

    // 选择后端
#ifdef GGML_USE_CUDA
    fprintf(stderr, "%s: using CUDA backend\n", __func__);
    model->backend = ggml_backend_cuda_init(0);
    if (!model->backend) {
        fprintf(stderr, "%s: ggml_backend_cuda_init() failed\n", __func__);
    }
#endif

#ifdef GGML_USE_METAL
    fprintf(stderr, "%s: using Metal backend\n", __func__);
    model->backend = ggml_backend_metal_init();
    if (!model->backend) {
        fprintf(stderr, "%s: ggml_backend_metal_init() failed\n", __func__);
    }
#endif

    if (!model->backend) {
        model->backend = ggml_backend_cpu_init();
    }

    struct ggml_init_params params = {
        .mem_size = ggml_tensor_overhead() * 2,
        .mem_buffer = NULL,
        .no_alloc = true,
    };

    model->ctx = ggml_init(params);

    model->a = ggml_new_tensor_2d(model->ctx, GGML_TYPE_F32, cols_A, rows_A);
    model->b = ggml_new_tensor_2d(model->ctx, GGML_TYPE_F32, cols_B, rows_B);

    model->buffer = ggml_backend_alloc_ctx_tensors(model->ctx, model->backend);

    ggml_backend_tensor_set(model->a, a, 0, ggml_nbytes(model->a));
    ggml_backend_tensor_set(model->b, b, 0, ggml_nbytes(model->b));
}

// 构建计算图
struct ggml_cgraph *build_graph(const struct simple_model *model) {
    static size_t buf_size = 4096 * 32;
    static uint8_t *buf = NULL;

    if (buf == NULL) {
        buf = (uint8_t *) malloc(buf_size);
    }

    struct ggml_init_params params = {
        .mem_size = buf_size,
        .mem_buffer = buf,
        .no_alloc = true,
    };

    struct ggml_context *ctx0 = ggml_init(params);
    struct ggml_cgraph *gf = ggml_new_graph(ctx0);

    struct ggml_tensor *result = ggml_mul_mat(ctx0, model->a, model->b);
    ggml_build_forward_expand(gf, result);

    // 释放临时 ctx（不释放 graph）
    ggml_free(ctx0);

    return gf;
}

// 计算执行
struct ggml_tensor *compute(const struct simple_model *model, ggml_gallocr_t allocr) {
    struct ggml_cgraph *gf = build_graph(model);
    ggml_gallocr_alloc_graph(allocr, gf);

    if (ggml_backend_is_cpu(model->backend)) {
        ggml_backend_cpu_set_n_threads(model->backend, 1);
    }

    ggml_backend_graph_compute(model->backend, gf);

    return ggml_graph_node(gf, -1);
}

#define ROWS_A 4
#define COLS_A 2
float matrix_A[ROWS_A * COLS_A] = {
    2, 8,
    5, 1,
    4, 2,
    8, 6
};

#define ROWS_B 3
#define COLS_B 2
float matrix_B[ROWS_B * COLS_B] = {
    10, 5,
    9, 9,
    5, 4
};

int main(void) {
    ggml_time_init();

    struct simple_model model = {0};
    load_model(&model, matrix_A, matrix_B, ROWS_A, COLS_A, ROWS_B, COLS_B);

    ggml_gallocr_t allocr = NULL;
    {
        allocr = ggml_gallocr_new(ggml_backend_get_default_buffer_type(model.backend));
        struct ggml_cgraph *gf = build_graph(&model);
        ggml_gallocr_reserve(allocr, gf);
        size_t mem_size = ggml_gallocr_get_buffer_size(allocr, 0);
        fprintf(stderr, "%s: compute buffer size: %.4f KB\n", __func__, mem_size / 1024.0);
    }

    struct ggml_tensor *result = compute(&model, allocr);

    size_t numel = ggml_nelements(result);
    float *out_data = (float *) malloc(numel * sizeof(float));
    ggml_backend_tensor_get(result, out_data, 0, ggml_nbytes(result));

    printf("mul mat (%d x %d) (transposed result):\n[", (int) result->ne[0], (int) result->ne[1]);
    for (int j = 0; j < result->ne[1]; ++j) {
        if (j > 0) printf("\n");
        for (int i = 0; i < result->ne[0]; ++i) {
            printf(" %.2f", out_data[j * result->ne[0] + i]);
        }
    }
    printf(" ]\n");

    free(out_data);
    ggml_gallocr_free(allocr);
    ggml_free(model.ctx);
    ggml_backend_buffer_free(model.buffer);
    ggml_backend_free(model.backend);

    return 0;
}
