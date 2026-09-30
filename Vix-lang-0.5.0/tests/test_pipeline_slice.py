import shutil
import subprocess


def test_pipeline_and_string_slice_codegen(compiler, tmp_path, root_dir):
    if shutil.which("clang") is None:
        raise AssertionError("clang is required to run LLVM IR codegen test")

    source = root_dir / "tests" / "regression" / "test407.vix"
    ir = tmp_path / "test407.ll"
    binary = tmp_path / "test407"

    compile_res = subprocess.run(
        [str(compiler), str(source), "-ll", str(ir)],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert compile_res.returncode == 0, compile_res.stderr

    clang_res = subprocess.run(
        ["clang", "-O0", str(ir), "-o", str(binary)],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert clang_res.returncode == 0, clang_res.stderr

    run_res = subprocess.run(
        [str(binary)],
        capture_output=True,
        text=True,
        timeout=5,
    )
    assert run_res.returncode == 0, run_res.stderr
    assert run_res.stdout.splitlines() == ["14", "bcd", "abcdef", "ef"]
