import subprocess

from control_flow import run_vixc0, vixc0_binary


def test_vixc0_parses_extern_block_ast(vixc0_binary):
    src = '''
extern "C" {
    fn puts(text: string): i32
}

fn main(): i32 {
    return 0
}
'''

    result = run_vixc0(vixc0_binary, src, "--ast")

    assert result.returncode == 0, result.stderr
    assert "externs:" in result.stdout
    assert "name: puts" in result.stdout
    assert "type: string" in result.stdout


def test_vixc0_codegen_extern_puts(vixc0_binary, tmp_path):
    src = '''
extern "C" {
    fn puts(text: string): i32
}

fn main(): i32 {
    puts("extern ok")
    return 0
}
'''

    result = run_vixc0(vixc0_binary, src)
    assert result.returncode == 0, result.stderr
    assert "declare i32 @puts(ptr)" in result.stdout
    assert "call i32 @puts" in result.stdout

    ll_path = tmp_path / "extern_puts.ll"
    ll_path.write_text(result.stdout)
    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 0
    assert run.stdout.strip() == "extern ok"


def test_vixc0_rejects_duplicate_extern_and_function(vixc0_binary):
    src = '''
extern "C" {
    fn foo(): i32
}

fn foo(): i32 {
    return 1
}

fn main(): i32 {
    return foo()
}
'''

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 1
    assert "NameError" in result.stdout
    assert "function 'foo' redeclared as extern" in result.stdout
