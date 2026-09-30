import subprocess

from control_flow import run_vixc0, vixc0_binary


def test_vixc0_ast_marks_mutable_let(vixc0_binary):
    src = "fn main(): i32 { let mut value = 1 value = 2 return value }"

    result = run_vixc0(vixc0_binary, src, "--ast")

    assert result.returncode == 0, result.stderr
    assert "type: LetStatement" in result.stdout
    assert "name: value" in result.stdout
    assert "mutable: true" in result.stdout


def test_vixc0_rejects_assignment_to_immutable_let(vixc0_binary):
    src = "fn main(): i32 { let value = 1 value = 2 return value }"

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "\x1b[31m" in result.stdout
    assert "TypeError(" in result.stdout
    assert "cannot assign to immutable binding 'value'" in result.stdout


def test_vixc0_allows_assignment_to_mutable_let(vixc0_binary, tmp_path):
    src = "fn main(): i32 { let mut value = 1 value = value + 4 print(value) return value }"

    result = run_vixc0(vixc0_binary, src)

    assert result.returncode == 0, result.stderr
    ll_path = tmp_path / "mut_assignment.ll"
    ll_path.write_text(result.stdout)

    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 5
    assert run.stdout.strip() == "5"
