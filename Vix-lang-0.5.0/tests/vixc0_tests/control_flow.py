import subprocess
import shutil
import tempfile
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parent.parent.parent
VIXC0 = ROOT / "bootstrap" / "vixc0"
HOST_VIXC = ROOT / "build" / "vixc"


@pytest.fixture(scope="session")
def vixc0_binary():
    if not HOST_VIXC.exists():
        pytest.skip("build/vixc is required to build vixc0")
    if shutil.which("make") is None:
        pytest.skip("make is required to build vixc0")

    result = subprocess.run(
        ["make", "-C", str(ROOT / "bootstrap"), "clean", "bootstrap"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    if result.returncode != 0:
        pytest.fail(result.stdout + result.stderr)
    if not VIXC0.exists():
        pytest.fail("vixc0 build completed without producing bootstrap/vixc0")
    return VIXC0


def run_vixc0(vixc0: Path, source: str, *args: str):
    with tempfile.NamedTemporaryFile("w", suffix=".vix", delete=False) as src:
        src.write(source)
        src_path = src.name
    try:
        return subprocess.run(
            [str(vixc0), *args, src_path],
            capture_output=True,
            text=True,
            timeout=20,
        )
    finally:
        Path(src_path).unlink(missing_ok=True)


def test_vixc0_lexes_if_elif_else_keywords(vixc0_binary):
    src = "fn main(): i32 { if (0) { return 1 } elif (1) { return 2 } else { return 3 } }"

    result = run_vixc0(vixc0_binary, src, "--lex")

    assert result.returncode == 0, result.stderr
    assert "Token(kind=keyword, text='if')" in result.stdout
    assert "Token(kind=keyword, text='elif')" in result.stdout
    assert "Token(kind=keyword, text='else')" in result.stdout


def test_vixc0_parses_if_elif_else_ast(vixc0_binary):
    src = """
fn main(): i32 {
    if (0) {
        return 1
    }
    elif (1) {
        return 2
    }
    else {
        return 3
    }
}
"""

    result = run_vixc0(vixc0_binary, src, "--ast")

    assert result.returncode == 0, result.stderr
    assert "type: IfStatement" in result.stdout
    assert "then:" in result.stdout
    assert "else:" in result.stdout
    assert "type: ParseError" not in result.stdout


def test_vixc0_typechecks_if_condition(vixc0_binary):
    src = 'fn main(): i32 { if ("bad") { return 1 } else { return 0 } }'

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "TypeError" in result.stdout
    assert "if condition must be bool or i32" in result.stdout


def test_vixc0_codegen_executes_first_matching_if_branch(vixc0_binary, tmp_path):
    src = """
fn main(): i32 {
    let mut x = 0
    if (0) {
        x = 10
    }
    elif (2 > 1) {
        x = 20
    }
    else {
        x = 30
    }
    print(x)
    return x
}
"""

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr
    assert "define i32 @main()" in result.stdout
    assert "if.then" in result.stdout
    assert "if.else" in result.stdout

    ll_path = tmp_path / "if_elif_else.ll"
    ll_path.write_text(result.stdout)
    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)

    assert run.returncode == 20
    assert run.stdout.strip() == "20"


def test_vixc0_break_exits_loop_not_function(vixc0_binary, tmp_path):
    src = """
fn main(): i32 {
    let mut i = 0
    while (i < 5) {
        if (i == 2) {
            break
        }
        i += 1
    }
    print(77)
    return 0
}
"""

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr

    ll_path = tmp_path / "break_not_return.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.strip() == "77"


def test_vixc0_continue_jumps_to_loop_condition(vixc0_binary, tmp_path):
    src = """
fn main(): i32 {
    let mut i = 0
    let mut sum = 0
    while (i < 5) {
        i += 1
        if (i == 3) {
            continue
        }
        sum += i
    }
    print(sum)
    return 0
}
"""

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr

    ll_path = tmp_path / "continue_loop.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.strip() == "12"


def test_vixc0_rejects_break_outside_loop(vixc0_binary):
    src = "fn main(): i32 { break return 0 }"

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "TypeError" in result.stdout
    assert "'break' used outside loop" in result.stdout


def test_vixc0_parses_for_range_ast(vixc0_binary):
    src = "fn main(): i32 { for (i in 1 .. 4) { print(i) } return 0 }"

    result = run_vixc0(vixc0_binary, src, "--ast")

    assert result.returncode == 0, result.stderr
    assert "type: ForStatement" in result.stdout
    assert "iterator: i" in result.stdout
    assert "start:" in result.stdout
    assert "end:" in result.stdout


def test_vixc0_codegen_for_range_sum(vixc0_binary, tmp_path):
    src = """
fn main(): i32 {
    let mut sum = 0
    for (i in 0 .. 5) {
        sum += i
    }
    print(sum)
    return 0
}
"""

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr
    assert "for.cond" in result.stdout
    assert "for.body" in result.stdout
    assert "for.step" in result.stdout

    ll_path = tmp_path / "for_sum.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.strip() == "10"


def test_vixc0_codegen_for_break_continue(vixc0_binary, tmp_path):
    src = """
fn main(): i32 {
    for (i in 0 .. 8) {
        if (i == 2) {
            continue
        }
        if (i == 5) {
            break
        }
        print(i)
    }
    print(99)
    return 0
}
"""

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr

    ll_path = tmp_path / "for_break_continue.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.splitlines() == ["0", "1", "3", "4", "99"]


def test_vixc0_rejects_float_for_range(vixc0_binary):
    src = "fn main(): i32 { for (i in 0.0 .. 3.0) { print(i) } return 0 }"

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "TypeError" in result.stdout
    assert "for range start must be integer" in result.stdout
