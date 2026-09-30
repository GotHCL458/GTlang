import subprocess

from control_flow import run_vixc0, vixc0_binary


def test_vixc0_parses_struct_decl_and_literal_ast(vixc0_binary):
    src = """
type Point = struct {
    x: i32,
    y: i32
}

fn main(): i32 {
    let p = Point { x: 3, y: 4 }
    return p.x
}
"""

    result = run_vixc0(vixc0_binary, src, "--ast")

    assert result.returncode == 0, result.stderr
    assert "type: Struct" in result.stdout
    assert "name: Point" in result.stdout
    assert "type: StructLiteral" in result.stdout
    assert "type: FieldExpression" in result.stdout


def test_vixc0_codegen_struct_fields(vixc0_binary, tmp_path):
    src = """
type Point = struct {
    x: i32,
    y: i32
}

fn main(): i32 {
    let p = Point { x: 3, y: 4 }
    let sum = p.x + p.y
    print(sum)
    return sum
}
"""

    result = run_vixc0(vixc0_binary, src)

    assert result.returncode == 0, result.stderr
    assert "%Point = type { i32, i32 }" in result.stdout
    assert "extractvalue" in result.stdout

    ll_path = tmp_path / "struct_fields.ll"
    ll_path.write_text(result.stdout)

    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 7
    assert run.stdout.strip() == "7"


def test_vixc0_codegen_nested_struct_fields(vixc0_binary, tmp_path):
    src = """
type Point = struct {
    x: i32,
    y: i32
}

type Box = struct {
    point: Point,
    scale: i32
}

fn main(): i32 {
    let b = Box { point: Point { x: 5, y: 6 }, scale: 2 }
    let result = b.point.y * b.scale
    print(result)
    return result
}
"""

    result = run_vixc0(vixc0_binary, src)

    assert result.returncode == 0, result.stderr
    assert "%Box = type { %Point, i32 }" in result.stdout

    ll_path = tmp_path / "nested_struct_fields.ll"
    ll_path.write_text(result.stdout)

    verify = subprocess.run(["opt", "-passes=verify", "-disable-output", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert verify.returncode == 0, verify.stderr

    run = subprocess.run(["lli", str(ll_path)], capture_output=True, text=True, timeout=20)
    assert run.returncode == 12
    assert run.stdout.strip() == "12"


def test_vixc0_rejects_unknown_struct_field(vixc0_binary):
    src = """
type Point = struct { x: i32 }
fn main(): i32 {
    let p = Point { x: 1 }
    return p.y
}
"""

    result = run_vixc0(vixc0_binary, src, "--typeinfer")

    assert result.returncode == 0
    assert "TypeError(" in result.stdout
    assert "unknown field 'y' on struct 'Point'" in result.stdout
