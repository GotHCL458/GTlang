import subprocess

from control_flow import run_vixc0, vixc0_binary


def test_vixc0_prints_argv_index_values(vixc0_binary, tmp_path):
    src = """
fn main(argc: i32, argv: ptr): i32 {
    print(argv[0])
    print(argv[1])
    return argc
}
"""

    result = run_vixc0(vixc0_binary, src)

    assert result.returncode == 0, result.stderr
    assert "define i32 @main(i32" in result.stdout
    assert "getelementptr ptr" in result.stdout

    ll_path = tmp_path / "args.ll"
    ll_path.write_text(result.stdout)

    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path), "hello"], capture_output=True, text=True, timeout=20)
    assert run.returncode == 2
    assert run.stdout.splitlines() == [str(ll_path), "hello"]


def test_vixc0_rejects_indexing_non_pointer(vixc0_binary):
    src = "fn main(): i32 { let x = 1 print(x[0]) return 0 }"

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "TypeError(" in result.stdout
    assert "cannot index value of type 'i32'" in result.stdout
