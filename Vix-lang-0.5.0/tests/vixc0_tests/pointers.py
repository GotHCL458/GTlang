import subprocess

from control_flow import run_vixc0, vixc0_binary


def test_vixc0_ref_and_deref_local_i32(vixc0_binary, tmp_path):
    src = """
fn main(): i32 {
    let value = 10
    let ptr = ref value
    print(@ptr)
    return @ptr
}
"""

    result = run_vixc0(vixc0_binary, src)

    assert result.returncode == 0, result.stderr
    ll_path = tmp_path / "pointer_read.ll"
    ll_path.write_text(result.stdout)

    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 10
    assert run.stdout.strip() == "10"


def test_vixc0_address_alias_and_deref_store(vixc0_binary, tmp_path):
    src = """
fn main(): i32 {
    let value = 10
    let ptr = &value
    @ptr = 20
    print(value)
    return value
}
"""

    result = run_vixc0(vixc0_binary, src)

    assert result.returncode == 0, result.stderr
    ll_path = tmp_path / "pointer_store.ll"
    ll_path.write_text(result.stdout)

    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 20
    assert run.stdout.strip() == "20"


def test_vixc0_rejects_deref_non_pointer(vixc0_binary):
    src = "fn main(): i32 { let value = 10 print(@value) return 0 }"

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "TypeError(" in result.stdout
    assert "cannot dereference value of type 'i32'" in result.stdout
