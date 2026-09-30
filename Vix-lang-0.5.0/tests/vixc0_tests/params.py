import shutil
import subprocess
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parent.parent.parent
VIXC0 = ROOT / "bootstrap" / "vixc0"
HOST_VIXC = ROOT / "build" / "vixc"


@pytest.fixture(scope="session")
def vixc0_binary():
    if not HOST_VIXC.exists():
        pytest.skip("build/vixc is required to build vixc0")
    result = subprocess.run(
        ["make", "-C", str(ROOT / "bootstrap"), "bootstrap"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert VIXC0.exists()
    return VIXC0


def test_vixc0_accepts_pointer_function_params(vixc0_binary, tmp_path):
    if shutil.which("opt") is None:
        pytest.skip("opt is required to verify vixc0 LLVM IR")
    if shutil.which("lli") is None:
        pytest.skip("lli is required to execute vixc0 LLVM IR")

    ir = tmp_path / "swap.ll"
    source = ROOT / "examples" / "swap.vix"

    compile_result = subprocess.run(
        [str(vixc0_binary), str(source)],
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert compile_result.returncode == 0, compile_result.stdout + compile_result.stderr
    ir.write_text(compile_result.stdout)

    verify_result = subprocess.run(
        ["opt", "-passes=verify", "-disable-output", str(ir)],
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert verify_result.returncode == 0, verify_result.stdout + verify_result.stderr

    run_result = subprocess.run(
        ["lli", str(ir)],
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert run_result.returncode == 0, run_result.stdout + run_result.stderr
    assert run_result.stdout == "20\n10\n"


def test_vixc0_accepts_composite_type_syntax(vixc0_binary, tmp_path):
    if shutil.which("opt") is None:
        pytest.skip("opt is required to verify vixc0 LLVM IR")
    if shutil.which("lli") is None:
        pytest.skip("lli is required to execute vixc0 LLVM IR")

    source = tmp_path / "composite_types.vix"
    ir = tmp_path / "composite_types.ll"
    source.write_text(
        "type Holder = struct {\n"
        "    items: [i32],\n"
        "    maybe: ?i8,\n"
        "    value: &i32,\n"
        "}\n"
        "\n"
        "fn keep_array(xs: [i32]): i32 {\n"
        "    return 7\n"
        "}\n"
        "\n"
        "fn keep_option(opt: ?i8): i32 {\n"
        "    return 9\n"
        "}\n"
        "\n"
        "fn pointer_let(): i32 {\n"
        "    let x = 1\n"
        "    let p: &i32 = &x\n"
        "    return @p\n"
        "}\n"
        "\n"
        "fn main(): i32 {\n"
        "    print(pointer_let())\n"
        "    return 0\n"
        "}\n"
    )

    ast_result = subprocess.run(
        [str(vixc0_binary), "--ast", str(source)],
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert ast_result.returncode == 0, ast_result.stdout + ast_result.stderr
    assert "type: [i32]" in ast_result.stdout
    assert "type: Option[i8]" in ast_result.stdout
    assert "type: ptr" in ast_result.stdout

    compile_result = subprocess.run(
        [str(vixc0_binary), str(source)],
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert compile_result.returncode == 0, compile_result.stdout + compile_result.stderr
    ir.write_text(compile_result.stdout)

    verify_result = subprocess.run(
        ["opt", "-passes=verify", "-disable-output", str(ir)],
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert verify_result.returncode == 0, verify_result.stdout + verify_result.stderr

    run_result = subprocess.run(
        ["lli", str(ir)],
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert run_result.returncode == 0, run_result.stdout + run_result.stderr
    assert run_result.stdout == "1\n"
