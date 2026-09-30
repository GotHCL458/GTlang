#include "binaryen-c.h"
#include "WasmCodegen.h"
#include "libvixc_frontend.h"
#include <cassert>
#include <cstdio>
#include <vector>
#include <string>

static int tests_passed = 0;
static int tests_failed = 0;

void test_binaryen_basic() {
    fprintf(stderr, "test_binaryen_basic: creating module...\n"); fflush(stderr);
    BinaryenModuleRef mod = BinaryenModuleCreate();
    fprintf(stderr, "test_binaryen_basic: writing...\n"); fflush(stderr);
    BinaryenModuleAllocateAndWriteResult r = BinaryenModuleAllocateAndWrite(mod, nullptr);
    fprintf(stderr, "test_binaryen_basic: binary=%p bytes=%zu\n", r.binary, r.binaryBytes); fflush(stderr);
    if (!r.binary) {
        fprintf(stderr, "FAIL: null binary from BinaryenModuleAllocateAndWrite\n"); fflush(stderr);
        return;
    }
    assert(r.binaryBytes > 4);
    uint8_t *b = (uint8_t*)r.binary;
    fprintf(stderr, "  magic: %02x %02x %02x %02x\n", b[0], b[1], b[2], b[3]); fflush(stderr);
    assert(b[0] == 0x00);
    assert(b[1] == 0x61);
    assert(b[2] == 0x73);
    assert(b[3] == 0x6d);
    free(r.binary);
    BinaryenModuleDispose(mod);
    fprintf(stderr, "PASS: test_binaryen_basic (%zu bytes)\n", r.binaryBytes); fflush(stderr);
    tests_passed++;
}

void test_compile_to_wasm() {
    const char *source = "fn add(a: i32, b: i32): i32 {\n"
                         "    return a + b\n"
                         "}\n"
                         "fn main(): i32 {\n"
                         "    return add(1, 2)\n"
                         "}\n";

    fprintf(stderr, "test_compile_to_wasm: compiling source...\n"); fflush(stderr);
    CompileResult cr = vixc_compile_string(source);
    fprintf(stderr, "  error_count=%d root=%p\n", cr.error_count, cr.root); fflush(stderr);
    if (cr.error_count != 0 || !cr.root) {
        fprintf(stderr, "FAIL: frontend compilation failed\n"); fflush(stderr);
        tests_failed++;
        return;
    }

    WasmCodegen cg;
    std::vector<uint8_t> wasm_bytes;
    std::string error;
    fprintf(stderr, "test_compile_to_wasm: emitting WASM...\n"); fflush(stderr);
    bool ok = cg.emit(cr.root, wasm_bytes, error);
    fprintf(stderr, "  ok=%d bytes=%zu error=[%s]\n", ok, wasm_bytes.size(), error.c_str()); fflush(stderr);

    if (!ok) {
        fprintf(stderr, "FAIL: emit() returned false\n"); fflush(stderr);
        tests_failed++;
        vixc_free_result(&cr);
        return;
    }
    assert(!wasm_bytes.empty());
    assert(wasm_bytes[0] == 0x00);
    assert(wasm_bytes[1] == 0x61);
    assert(wasm_bytes[2] == 0x73);
    assert(wasm_bytes[3] == 0x6d);

    vixc_free_result(&cr);
    fprintf(stderr, "PASS: test_compile_to_wasm (%zu bytes)\n", wasm_bytes.size()); fflush(stderr);
    tests_passed++;
}

int main() {
    fprintf(stderr, "=== WASM Codegen Test ===\n");
    test_binaryen_basic();
    test_compile_to_wasm();
    fprintf(stderr, "\n%d passed, %d failed\n", tests_passed, tests_failed);

    return tests_failed > 0 ? 1 : 0;
}
