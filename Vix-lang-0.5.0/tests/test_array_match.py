import subprocess

import pytest

from helpers import compile_and_run


@pytest.mark.integration
@pytest.mark.parametrize(
    ("array", "pattern", "matched", "expected"),
    [
        ("[]", "[]", 'print("Empty")', "Empty"),
        ("[10, 20]", "[first, last]", "print(first, last)", "1020"),
        ("[10, 20, 30]", "[first, last]", "print(first, last)", "Other"),
        ("[7, 8]", "[first, .., last]", "print(first, last)", "78"),
        ("[7]", "[first, .., last]", "print(first, last)", "Other"),
        ("[1, 2, 3]", "[1, .., 3]", 'print("Matched")', "Matched"),
        ("[1, 2, 4]", "[1, .., 3]", 'print("Matched")', "Other"),
    ],
)
def test_array_match_patterns(compiler, tmp_path, array, pattern, matched, expected):
    source = f"""
fn main(): i32 {{
    let nums = {array}
    match nums {{
        {pattern} -> {matched}
        _ -> print("Other")
    }}
    return 0
}}
"""

    compiled, run = compile_and_run(compiler, source, tmp_path)

    assert compiled.returncode == 0, compiled.stderr
    assert run is not None
    assert run.returncode == 0
    assert run.stdout.strip() == expected


@pytest.mark.integration
@pytest.mark.parametrize(
    ("setup", "target", "expected"),
    [
        ("", "[1, 2, 3]", "13"),
        ("let mut nums = [] nums.push(7) nums.push(8)", "nums", "78"),
    ],
)
def test_array_match_materialized_and_dynamic_targets(
    compiler, tmp_path, setup, target, expected
):
    source = f"""
fn main(): i32 {{
    {setup}
    match {target} {{
        [first, .., last] -> print(first, last)
        _ -> print("Other")
    }}
    return 0
}}
"""

    compiled, run = compile_and_run(compiler, source, tmp_path)

    assert compiled.returncode == 0, compiled.stderr
    assert run is not None
    assert run.returncode == 0
    assert run.stdout.strip() == expected


@pytest.mark.integration
def test_array_match_binding_shadows_without_mutating_outer_value(compiler, tmp_path):
    source = """
fn main(): i32 {
    let first = 99
    let nums = [1, 2]
    match nums {
        [first, ..] -> print(first)
        _ -> print("Other")
    }
    print(first)
    return 0
}
"""

    compiled, run = compile_and_run(compiler, source, tmp_path)

    assert compiled.returncode == 0, compiled.stderr
    assert run is not None
    assert run.returncode == 0
    assert run.stdout.splitlines() == ["1", "99"]


@pytest.mark.integration
def test_array_match_llvm_ir_verifies(compiler, tmp_path):
    source = tmp_path / "array_match.vix"
    llvm_ir = tmp_path / "array_match.ll"
    source.write_text(
        """
fn main() {
    let nums = [10, 20, 30, 40, 50]
    match nums {
        [] -> print("Empty")
        [first, .., last] -> print("First:", first, "Last:", last)
        _ -> print("Other")
    }
}
"""
    )

    compiled = subprocess.run(
        [str(compiler), str(source), "-ll", str(llvm_ir)],
        capture_output=True,
        text=True,
        timeout=30,
    )

    assert compiled.returncode == 0, compiled.stderr
    assert llvm_ir.exists()
    verified = subprocess.run(
        ["opt", "-passes=verify", "-disable-output", str(llvm_ir)],
        capture_output=True,
        text=True,
        timeout=20,
    )
    assert verified.returncode == 0, verified.stderr


@pytest.mark.integration
def test_array_match_rejects_multiple_rest_patterns(compiler, tmp_path):
    source = tmp_path / "invalid_array_match.vix"
    source.write_text(
        """
fn main() {
    let nums = [1, 2, 3]
    match nums {
        [first, .., .., last] -> print(first, last)
        _ -> print("Other")
    }
}
"""
    )

    checked = subprocess.run(
        [str(compiler), str(source), "--check"],
        capture_output=True,
        text=True,
        timeout=30,
    )

    assert checked.returncode != 0
    assert "array pattern may contain at most one '..'" in checked.stderr
