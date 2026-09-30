#include "../src/libvixc_frontend.h"
#include <assert.h>
#include <string.h>
#include <stdio.h>

void test_compile_ok() {
    const char *source = "fn main(): i32 { print(\"hello\"); return 0 }\n";
    CompileResult r = vixc_compile_string(source);
    assert(r.error_count == 0);
    assert(r.root != NULL);
    vixc_free_result(&r);
    printf("PASS: test_compile_ok\n");
}

void test_syntax_error() {
    const char *source = "fn main() { this is bad syntax @@@ }\n";
    CompileResult r = vixc_compile_string(source);
    assert(r.error_count > 0);
    printf("PASS: test_syntax_error\n");
}

void test_type_error() {
    const char *source = "fn main(): i32 { return \"string\"; }\n";
    CompileResult r = vixc_compile_string(source);
    assert(r.error_count > 0);
    printf("PASS: test_type_error\n");
}

void test_compile_macro() {
    const char *source =
        "macro $identity(value: expr) { $value }\n"
        "fn main(): i32 { return $identity(0) }\n";
    CompileResult r = vixc_compile_string(source);
    assert(r.error_count == 0);
    assert(r.root != NULL);
    vixc_free_result(&r);
    printf("PASS: test_compile_macro\n");
}

int main() {
    test_compile_ok();
    test_syntax_error();
    test_type_error();
    test_compile_macro();
    printf("All frontend tests passed\n");
    return 0;
}
