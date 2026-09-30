import subprocess

from helpers import compile_vix


def test_macro_example_compiles_and_runs(compiler, root_dir, tmp_path):
    output = tmp_path / "macro_example"
    compiled = compile_vix(
        compiler, root_dir / "examples" / "macro.vix", output
    )
    assert compiled.returncode == 0, compiled.stderr

    run = subprocess.run(
        [str(output)], capture_output=True, text=True, timeout=5
    )
    assert run.returncode == 0, run.stderr
    assert run.stdout == "30\n1 2 3 4 "


def test_expression_macro_supports_nesting_and_repetition(
    compiler, tmp_path
):
    source = tmp_path / "nested_macros.vix"
    source.write_text(
        """
macro $twice(value: expr) {
    ($value) + ($value)
}

macro $sum[values: expr*] {
    0 $(+ values)*
}

macro $array[values: expr*] {
    [$($values),*]
}

fn main(): i32 {
    print($twice($sum[1, (2 + 3)]))
    let values = $array[3, 5, 8]
    print(values[2])
    return 0
}
"""
    )
    output = tmp_path / "nested_macros"
    compiled = compile_vix(compiler, source, output)
    assert compiled.returncode == 0, compiled.stderr

    run = subprocess.run(
        [str(output)], capture_output=True, text=True, timeout=5
    )
    assert run.returncode == 0, run.stderr
    assert run.stdout == "12\n8\n"


def test_macro_text_in_strings_and_comments_is_not_expanded(
    compiler, tmp_path
):
    source = tmp_path / "macro_literals.vix"
    source.write_text(
        """
// $missing(comment)
fn main(): i32 {
    print("$missing(string)")
    return 0
}
"""
    )
    output = tmp_path / "macro_literals"
    compiled = compile_vix(compiler, source, output)
    assert compiled.returncode == 0, compiled.stderr

    run = subprocess.run(
        [str(output)], capture_output=True, text=True, timeout=5
    )
    assert run.returncode == 0, run.stderr
    assert run.stdout == "$missing(string)\n"


def test_macro_can_call_a_later_macro(compiler, tmp_path):
    source = tmp_path / "forward_macro.vix"
    source.write_text(
        """
macro $outer(value: expr) {
    $inner($value)
}

macro $inner(value: expr) {
    ($value) + 1
}

fn main(): i32 {
    print($outer(4))
    return 0
}
"""
    )
    output = tmp_path / "forward_macro"
    compiled = compile_vix(compiler, source, output)
    assert compiled.returncode == 0, compiled.stderr
    run = subprocess.run(
        [str(output)], capture_output=True, text=True, timeout=5
    )
    assert run.returncode == 0, run.stderr
    assert run.stdout == "5\n"


def test_recursive_macro_hits_expansion_limit(compiler, tmp_path):
    source = tmp_path / "recursive_macro.vix"
    source.write_text(
        """
macro $forever() {
    $forever()
}

fn main(): i32 {
    return $forever()
}
"""
    )
    checked = subprocess.run(
        [str(compiler), str(source), "--check"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert checked.returncode != 0
    assert "MacroError" in checked.stderr
    assert "recursion limit (64)" in checked.stderr


def test_ident_fragment_rejects_expression(compiler, tmp_path):
    source = tmp_path / "invalid_ident_macro.vix"
    source.write_text(
        """
macro $declare(name: ident) {
    let $name = 1
}

fn main(): i32 {
    $declare(1 + 2)
    return 0
}
"""
    )
    checked = subprocess.run(
        [str(compiler), str(source), "--check"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert checked.returncode != 0
    assert "MacroError" in checked.stderr
    assert "must be an identifier" in checked.stderr
    assert f"{source}:7:5" in checked.stderr


def test_imported_module_expands_its_macros(compiler, tmp_path):
    module = tmp_path / "generated_module.vix"
    module.write_text(
        """
macro $make_function(name: ident) {
    pub fn $name(): i32 {
        return 41
    }
}

$make_function(answer)
"""
    )
    source = tmp_path / "import_macro.vix"
    source.write_text(
        """
import "generated_module.vix"

fn main(): i32 {
    print(answer())
    return 0
}
"""
    )
    output = tmp_path / "import_macro"
    compiled = compile_vix(compiler, source, output)
    assert compiled.returncode == 0, compiled.stderr

    run = subprocess.run(
        [str(output)], capture_output=True, text=True, timeout=5
    )
    assert run.returncode == 0, run.stderr
    assert run.stdout == "41\n"


def test_imported_module_macro_error_fails_compilation(compiler, tmp_path):
    module = tmp_path / "broken_module.vix"
    module.write_text(
        """
macro $make_function(name: ident) {
    pub fn $name(): i32 { return 0 }
}

$make_function(1 + 2)
"""
    )
    source = tmp_path / "import_broken_macro.vix"
    source.write_text(
        """
import "broken_module.vix"
fn main(): i32 { return 0 }
"""
    )
    checked = subprocess.run(
        [str(compiler), str(source), "--check"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert checked.returncode != 0
    assert "MacroError" in checked.stderr
    assert "must be an identifier" in checked.stderr
