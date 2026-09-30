import subprocess

import pytest

from control_flow import run_vixc0, vixc0_binary


def test_vixc0_lexes_float_literal(vixc0_binary):
    src = "fn main(): i32 { let x: f64 = 3.5 print(x) return 0 }"

    result = run_vixc0(vixc0_binary, src, "--lex")

    assert result.returncode == 0, result.stderr
    assert "Token(kind=float, text='3.5', value=3.5)" in result.stdout


def test_vixc0_typeinfer_accepts_basic_types_and_alias(vixc0_binary):
    src = (
        'fn main(): i32 { let a: i64 = 42 let b: f64 = 3.5 '
        'let c: f32 = 1.5 let s: string = "ok" let t: bool = true return 0 }'
    )

    canonical = run_vixc0(vixc0_binary, src, "--typeinfer")
    alias = run_vixc0(vixc0_binary, src, "--tyinfer")

    assert canonical.returncode == 0
    assert canonical.stdout.strip() == "TypeOk(return_type=i32)"
    assert alias.returncode == 0
    assert alias.stdout.strip() == canonical.stdout.strip()


@pytest.mark.parametrize(
    "src, message",
    [
        ("fn main(): i32 { let s: string = 1 return 0 }", "cannot initialize 's' of type 'string' with 'i32'"),
        ('fn main(): i32 { let b: bool = "bad" return 0 }', "cannot initialize 'b' of type 'bool' with 'string'"),
    ],
)
def test_vixc0_typeinfer_rejects_basic_type_mismatch(vixc0_binary, src, message):
    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "TypeError(" in result.stdout
    assert message in result.stdout


def test_vixc0_codegen_basic_types_verify_and_run(vixc0_binary, tmp_path):
    src = '''
fn main(): i32 {
    let a: i64 = 42
    let b: f64 = 3.5
    let c: f32 = 1.5
    let s: string = "ok"
    let t: bool = true
    print(a)
    print(b)
    print(c)
    print(s)
    print(t)
    return 0
}
'''

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr
    assert "alloca i64" in result.stdout
    assert "alloca double" in result.stdout
    assert "alloca float" in result.stdout
    assert "alloca ptr" in result.stdout
    assert "call i32 (ptr, ...) @printf" in result.stdout

    ll_path = tmp_path / "basic_types.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.splitlines() == ["42", "3.500000", "1.500000", "ok", "1"]


def test_vixc0_codegen_float_comparison(vixc0_binary, tmp_path):
    src = "fn main(): i32 { let x: f64 = 1.5 if (x < 2.0) { print(1) } else { print(0) } return 0 }"

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr

    ll_path = tmp_path / "float_compare.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.strip() == "1"


def test_vixc0_codegen_call_return_types_f64_and_bool(vixc0_binary, tmp_path):
    src = """
fn amount(): f64 {
    return 4.5
}

fn ready(): bool {
    return true
}

fn main(): i32 {
    let x: f64 = amount()
    print(x)
    if (ready()) {
        print(1)
    } else {
        print(0)
    }
    return 0
}
"""

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr
    assert "call double @amount" in result.stdout
    assert "call i32 @ready" in result.stdout

    ll_path = tmp_path / "call_return_types.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.splitlines() == ["4.500000", "1"]


def test_vixc0_codegen_bitwise_integer_ops(vixc0_binary, tmp_path):
    src = """
fn main(): i32 {
    let one = 1
    let masked = (one + 239) & (one + 14)
    let shifted = one << 5
    let combined = masked | shifted
    let result = combined + (64 >> (one + 3))
    print(result)
    return result
}
"""

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr
    assert " and " in result.stdout
    assert " or " in result.stdout
    assert " shl " in result.stdout
    assert " ashr " in result.stdout

    ll_path = tmp_path / "bitwise_ops.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 36
    assert run.stdout.strip() == "36"


def test_vixc0_typed_i8_pointer_index_supports_byte_arithmetic(vixc0_binary, tmp_path):
    src = """
extern "C"
{
    fn malloc(size: usize): &i8
    fn free(buf: ptr): void
}

fn main(): i32 {
    let buf: &i8 = malloc(4)
    buf[0] = 65
    buf[1] = 255
    let a = buf[0] & 255
    let b = buf[1] & 255
    print(a)
    print(b)
    free(buf)
    return 0
}
"""

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr
    assert "load i8" in result.stdout
    assert "zext i8" in result.stdout
    assert " and " in result.stdout

    ll_path = tmp_path / "typed_i8_pointer_index.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.splitlines() == ["65", "255"]
