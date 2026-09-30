#include <emscripten.h>
#include "../src/libvixc_frontend.h"
#include "../src/compiler/WasmCodegen.h"
#include <string>
#include <vector>
#include <cstring>

extern "C" {

EMSCRIPTEN_KEEPALIVE
int compile_vix(const char *source, char **out_wasm_bytes, int *out_wasm_len, char **out_error) {
    CompileResult cr = vixc_compile_string(source);
    if (cr.error_count > 0 || !cr.root) {
        *out_error = strdup(vixc_get_last_error());
        return 0;
    }

    WasmCodegen cg;
    std::vector<uint8_t> wasm_bytes;
    std::string error;
    if (!cg.emit(cr.root, wasm_bytes, error)) {
        *out_error = strdup(error.c_str());
        vixc_free_result(&cr);
        return 0;
    }

    *out_wasm_len = wasm_bytes.size();
    *out_wasm_bytes = (char*)malloc(wasm_bytes.size());
    memcpy(*out_wasm_bytes, wasm_bytes.data(), wasm_bytes.size());

    vixc_free_result(&cr);
    return 1;
}

EMSCRIPTEN_KEEPALIVE
void free_wasm_result(char *bytes, char *error) {
    if (bytes) free(bytes);
    if (error) free(error);
}

}
