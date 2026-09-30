"""
500+ fuzz tests for the Vix compiler.
Each test generates a random Vix program and verifies the compiler doesn't crash (SIGSEGV, SIGABRT, etc.).
Compilation errors are acceptable; crashes are not.
"""
import random
import signal
import string
import subprocess
import sys
from pathlib import Path

import pytest
sys.path.insert(0, str(Path(__file__).resolve().parent))
from helpers import COMPILER

CRASH_SIGNALS = {signal.SIGSEGV, signal.SIGABRT, signal.SIGFPE, signal.SIGILL, signal.SIGBUS}


def rand_int(min_val=-1000, max_val=1000):
    return str(random.randint(min_val, max_val))

def rand_float():
    return f"{random.uniform(-100.0, 100.0):.6f}"

def rand_string():
    length = random.randint(0, 20)
    chars = random.choices(string.ascii_lowercase + string.digits, k=length)
    return '"' + "".join(chars) + '"'

def rand_ident(prefix="v"):
    return f"{prefix}{random.randint(0, 9999)}"

def rand_type():
    return random.choice(["i8", "i32", "i64", "f32", "f64", "bool", "str"])

def rand_literal():
    return random.choice([rand_int, rand_float, rand_string, lambda: "true", lambda: "false", lambda: "nil"])()

def rand_simple_expr(depth=0):
    if depth > 2:
        return rand_literal()
    kind = random.choice(["lit", "binop", "ident", "paren"])
    if kind == "lit":
        return rand_literal()
    elif kind == "binop":
        op = random.choice(["+", "-", "*", "/", "%", "**"])
        return f"({rand_simple_expr(depth+1)} {op} {rand_simple_expr(depth+1)})"
    elif kind == "ident":
        return rand_ident()
    else:
        return f"({rand_simple_expr(depth+1)})"


def generate_random_let():
    name = rand_ident()
    t = rand_type()
    val = rand_literal()
    use_type = random.choice([True, False])
    mut = random.choice([True, False])
    if use_type:
        return f"let {'mut ' if mut else ''}{name}: {t} = {val}"
    return f"let {'mut ' if mut else ''}{name} = {val}"


def generate_random_print():
    return f"print({rand_simple_expr()})"


def generate_random_if(depth=0):
    if depth > 2:
        return f"if ({rand_simple_expr()}) {{ print({rand_literal()}) }}"
    cond = rand_simple_expr()
    has_else = random.choice([True, False])
    body = generate_random_statement(depth + 1)
    if has_else:
        else_body = generate_random_statement(depth + 1)
        return f"if ({cond}) {{ {body} }} else {{ {else_body} }}"
    return f"if ({cond}) {{ {body} }}"


def generate_random_while(depth=0):
    cond = random.choice(["0", "1", rand_simple_expr()])
    body = generate_random_statement(depth + 1)
    return f"while ({cond}) {{ {body} }}"


def generate_random_for():
    var = rand_ident("i")
    lo = random.randint(0, 5)
    hi = random.randint(lo, lo + 10)
    body = f"print({var})"
    return f"for ({var} in {lo} .. {hi}) {{ {body} }}"


def generate_random_statement(depth=0):
    kind = random.choice(["let", "print", "assign", "if", "expr"])
    if kind == "let":
        return generate_random_let()
    elif kind == "print":
        return generate_random_print()
    elif kind == "assign":
        return f"{rand_ident()} = {rand_simple_expr()}"
    elif kind == "if" and depth < 3:
        return generate_random_if(depth)
    else:
        return f"print({rand_simple_expr()})"


def generate_random_program(num_stmts=None):
    if num_stmts is None:
        num_stmts = random.randint(1, 20)
    stmts = []
    for _ in range(num_stmts):
        stmts.append("    " + generate_random_statement())
    body = "\n".join(stmts)
    return f"fn main(): i32 {{\n{body}\n    return 0\n}}"


def generate_random_function():
    name = rand_ident("fn_")
    num_params = random.randint(0, 3)
    params = []
    for i in range(num_params):
        pname = rand_ident(f"p{i}")
        params.append(f"{pname}: {rand_type()}")
    param_str = ", ".join(params)
    ret_type = random.choice(["i32", "void"])
    num_stmts = random.randint(1, 5)
    stmts = []
    for _ in range(num_stmts):
        stmts.append("    " + generate_random_statement())
    if ret_type != "void":
        stmts.append("    return " + rand_simple_expr())
    body = "\n".join(stmts)
    return f"fn {name}({param_str}) -> {ret_type} {{\n{body}\n}}"


def generate_multi_function_program():
    num_fns = random.randint(1, 5)
    fns = []
    for _ in range(num_fns):
        fns.append(generate_random_function())
    main_fn = generate_random_program(random.randint(1, 10))
    return "\n".join(fns) + "\n" + main_fn


def generate_random_struct():
    name = rand_ident("S")
    num_fields = random.randint(1, 5)
    fields = []
    for i in range(num_fields):
        fname = rand_ident(f"f{i}")
        fields.append(f"    {fname}: {rand_type()}")
    fields_str = ",\n".join(fields)
    return f"struct {name} {{\n{fields_str}\n}}"


def generate_struct_program():
    num_structs = random.randint(1, 3)
    structs = []
    for _ in range(num_structs):
        structs.append(generate_random_struct())
    main_fn = generate_random_program(random.randint(1, 5))
    return "\n".join(structs) + "\n" + main_fn


def generate_match_program():
    scrutinee = rand_ident("x")
    num_arms = random.randint(2, 6)
    arms = []
    for i in range(num_arms - 1):
        arms.append(f"        {i} -> {{ print({i}) }}")
    arms.append(f"        _ -> {{ print(-1) }}")
    arms_str = "\n".join(arms)
    return f"""fn main(): i32 {{
    let {scrutinee} = {random.randint(0, num_arms + 2)}
    match {scrutinee} {{
{arms_str}
    }}
    return 0
}}"""


def generate_adt_match_program():
    choice = random.choice(["option", "result"])
    if choice == "option":
        val = random.choice(["Some(42)", "None"])
        return f"""fn main(): i32 {{
    let x = {val}
    match x {{
        Some(v) -> {{ print(v) }}
        None -> {{ print(0) }}
    }}
    return 0
}}"""
    else:
        val = random.choice(["Ok(100)", "Err(\"fail\")"])
        return f"""fn main(): i32 {{
    let x = {val}
    match x {{
        Ok(v) -> {{ print(v) }}
        Err(e) -> {{ print(-1) }}
    }}
    return 0
}}"""


def generate_power_program():
    base = random.randint(0, 10)
    exp = random.randint(0, 10)
    return f"""fn main(): i32 {{
    let result = {base} ** {exp}
    print(result)
    return 0
}}"""


def generate_string_match_program():
    strings = ["hello", "world", "foo", "bar", "test", "abc"]
    chosen = random.choice(strings)
    arms = []
    for s in strings[:4]:
        arms.append(f'        "{s}" -> {{ print("{s}") }}')
    arms.append(f'        _ -> {{ print("other") }}')
    arms_str = "\n".join(arms)
    return f"""fn main(): i32 {{
    let s = "{chosen}"
    match s {{
{arms_str}
    }}
    return 0
}}"""


def run_compiler(program_src):
    """Compile a program and check for crashes."""
    import tempfile, os
    with tempfile.NamedTemporaryFile(suffix=".vix", mode="w", delete=False) as f:
        f.write(program_src)
        src_path = f.name
    out_path = src_path + ".bin"
    try:
        result = subprocess.run(
            [str(COMPILER), src_path, "-o", out_path],
            capture_output=True, text=True, timeout=15
        )
        if result.returncode < 0:
            sig = -result.returncode
            return False, f"Compiler crashed with signal {sig}"
        # Also try running if compilation succeeded
        if result.returncode == 0:
            try:
                run_result = subprocess.run(
                    [out_path], capture_output=True, text=True, timeout=5
                )
                if run_result.returncode < 0:
                    sig = -run_result.returncode
                    return False, f"Binary crashed with signal {sig}"
            except subprocess.TimeoutExpired:
                pass  # timeout is OK for fuzz testing
        return True, "OK"
    except subprocess.TimeoutExpired:
        return True, "Timeout (OK)"
    finally:
        try:
            os.unlink(src_path)
        except:
            pass
        try:
            os.unlink(out_path)
        except:
            pass


# ============================================================
# Fuzz Tests - Random Programs (100 tests via parametrize)
# ============================================================
@pytest.mark.fuzz
class TestFuzzRandomPrograms:
    @pytest.mark.parametrize("seed", range(100))
    def test_random_program_no_crash(self, seed):
        random.seed(seed + 42)
        program = generate_random_program()
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"


# ============================================================
# Fuzz Tests - Multi-Function Programs (80 tests)
# ============================================================
@pytest.mark.fuzz
class TestFuzzMultiFunction:
    @pytest.mark.parametrize("seed", range(80))
    def test_multi_function_no_crash(self, seed):
        random.seed(seed + 1000)
        program = generate_multi_function_program()
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"


# ============================================================
# Fuzz Tests - Expressions (80 tests)
# ============================================================
@pytest.mark.fuzz
class TestFuzzExpressions:
    @pytest.mark.parametrize("seed", range(80))
    def test_random_expressions_no_crash(self, seed):
        random.seed(seed + 2000)
        num_exprs = random.randint(3, 15)
        stmts = []
        for _ in range(num_exprs):
            expr = rand_simple_expr(depth=0)
            stmts.append(f"    print({expr})")
        body = "\n".join(stmts)
        program = f"fn main(): i32 {{\n{body}\n    return 0\n}}"
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"


# ============================================================
# Fuzz Tests - Struct Programs (50 tests)
# ============================================================
@pytest.mark.fuzz
class TestFuzzStructs:
    @pytest.mark.parametrize("seed", range(50))
    def test_struct_program_no_crash(self, seed):
        random.seed(seed + 3000)
        program = generate_struct_program()
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"


# ============================================================
# Fuzz Tests - Match Programs (60 tests)
# ============================================================
@pytest.mark.fuzz
class TestFuzzMatch:
    @pytest.mark.parametrize("seed", range(30))
    def test_match_integer_no_crash(self, seed):
        random.seed(seed + 4000)
        program = generate_match_program()
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"

    @pytest.mark.parametrize("seed", range(30))
    def test_match_adt_no_crash(self, seed):
        random.seed(seed + 5000)
        program = generate_adt_match_program()
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"


# ============================================================
# Fuzz Tests - Power Operator (50 tests)
# ============================================================
@pytest.mark.fuzz
class TestFuzzPower:
    @pytest.mark.parametrize("seed", range(50))
    def test_power_no_crash(self, seed):
        random.seed(seed + 6000)
        program = generate_power_program()
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"


# ============================================================
# Fuzz Tests - String Match (50 tests)
# ============================================================
@pytest.mark.fuzz
class TestFuzzStringMatch:
    @pytest.mark.parametrize("seed", range(50))
    def test_string_match_no_crash(self, seed):
        random.seed(seed + 7000)
        program = generate_string_match_program()
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"


# ============================================================
# Fuzz Tests - Type Annotations (50 tests)
# ============================================================
@pytest.mark.fuzz
class TestFuzzTypeAnnotations:
    @pytest.mark.parametrize("seed", range(50))
    def test_typed_let_no_crash(self, seed):
        random.seed(seed + 8000)
        types = ["i8", "i32", "i64", "f32", "f64", "bool", "str"]
        values = ["42", "3.14", "true", '"hello"', "0", "-1", "0xFF"]
        num_stmts = random.randint(3, 10)
        stmts = []
        for _ in range(num_stmts):
            t = random.choice(types)
            v = random.choice(values)
            name = rand_ident()
            stmts.append(f"    let {name}: {t} = {v}")
        body = "\n".join(stmts)
        program = f"fn main(): i32 {{\n{body}\n    return 0\n}}"
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"


# ============================================================
# Fuzz Tests - Mixed Operations (40 tests)
# ============================================================
@pytest.mark.fuzz
class TestFuzzMixed:
    @pytest.mark.parametrize("seed", range(40))
    def test_mixed_operations_no_crash(self, seed):
        random.seed(seed + 9000)
        templates = [
            lambda: generate_random_program(random.randint(2, 8)),
            lambda: generate_match_program(),
            lambda: generate_power_program(),
            lambda: generate_string_match_program(),
            lambda: generate_multi_function_program(),
        ]
        program = random.choice(templates)()
        ok, msg = run_compiler(program)
        assert ok, f"Seed {seed}: {msg}\nProgram:\n{program[:500]}"
