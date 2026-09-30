import subprocess

from control_flow import run_vixc0, vixc0_binary


def test_vixc0_parses_match_ast(vixc0_binary):
    src = """
fn main(): i32 {
    match 2 {
        1 -> { return 10 }
        2 -> { return 20 }
        _ -> { return 0 }
    }
}
"""

    result = run_vixc0(vixc0_binary, src, "--ast")

    assert result.returncode == 0, result.stderr
    assert "type: MatchStatement" in result.stdout
    assert "arms:" in result.stdout
    assert "type: Literal" in result.stdout


def test_vixc0_codegen_match_i32(vixc0_binary, tmp_path):
    src = """
fn classify(x: i32): i32 {
    match x {
        1 -> { return 10 }
        2 -> { return 20 }
        _ -> { return 0 }
    }
}

fn main(): i32 {
    let result = classify(2)
    print(result)
    return result
}
"""

    result = run_vixc0(vixc0_binary, src)

    assert result.returncode == 0, result.stderr
    assert "match.test" in result.stdout
    ll_path = tmp_path / "match_i32.ll"
    ll_path.write_text(result.stdout)

    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 20
    assert run.stdout.strip() == "20"


def test_vixc0_codegen_match_string_runtime(vixc0_binary, tmp_path):
    src = """
fn classify(name: string): i32 {
    match name {
        "apple" | "pear" -> { return 1 }
        "banana" -> { return 2 }
        _ -> { return 0 }
    }
}

fn main(): i32 {
    let result = classify("banana")
    print(result)
    return result
}
"""

    result = run_vixc0(vixc0_binary, src)

    assert result.returncode == 0, result.stderr
    assert "strcmp" in result.stdout
    ll_path = tmp_path / "match_string.ll"
    ll_path.write_text(result.stdout)

    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 2
    assert run.stdout.strip() == "2"


def test_vixc0_rejects_match_pattern_type_mismatch(vixc0_binary):
    src = """
fn main(): i32 {
    match 1 {
        "bad" -> { return 1 }
        _ -> { return 0 }
    }
}
"""

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "TypeError(" in result.stdout
    assert "match pattern type 'string' does not match subject type 'i32'" in result.stdout
