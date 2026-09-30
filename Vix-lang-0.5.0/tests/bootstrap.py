#!/usr/bin/env python3
"""Deterministic stability tests for bootstrap vixc0/vixc1 compilers."""

import argparse
import random
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_COMPILERS = (
    ROOT / "bootstrap" / "vixc0",
    ROOT / "bootstrap" / "vixc1",
)


@dataclass(frozen=True)
class Case:
    name: str
    source: str
    expected: str


def run(cmd, timeout=20):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def lines_output(values):
    return "".join(f"{value}\n" for value in values)


def regression_cases(count):
    cases = []
    for i in range(count):
        kind = i % 10
        if kind == 0:
            a = i + 3
            b = i * 2 + 1
            expected = a + b * 2
            src = f"""fn main(): i32 {{
    let a = {a}
    let b = {b}
    print(a + b * 2)
    return 0
}}"""
            cases.append(Case(f"reg_arith_{i:03}", src, lines_output([expected])))
        elif kind == 1:
            n = i % 17 + 3
            expected = sum(range(n))
            src = f"""fn main(): i32 {{
    let mut sum = 0
    for (j in 0 .. {n}) {{
        sum = sum + j
    }}
    print(sum)
    return 0
}}"""
            cases.append(Case(f"reg_for_sum_{i:03}", src, lines_output([expected])))
        elif kind == 2:
            n = i % 9 + 1
            expected = n * (n + 1) // 2
            src = f"""fn main(): i32 {{
    let mut n = {n}
    let mut sum = 0
    while (n > 0) {{
        sum = sum + n
        n = n - 1
    }}
    print(sum)
    return 0
}}"""
            cases.append(Case(f"reg_while_sum_{i:03}", src, lines_output([expected])))
        elif kind == 3:
            value = i % 5
            expected = 100 + value if value < 3 else 200 + value
            src = f"""fn main(): i32 {{
    let value = {value}
    if (value < 3) {{
        print(100 + value)
    }} else {{
        print(200 + value)
    }}
    return 0
}}"""
            cases.append(Case(f"reg_if_else_{i:03}", src, lines_output([expected])))
        elif kind == 4:
            a = i % 11
            b = i % 7
            src = f"""fn add(a: i32, b: i32): i32 {{
    return a + b
}}

fn main(): i32 {{
    print(add({a}, {b}))
    return 0
}}"""
            cases.append(Case(f"reg_call_{i:03}", src, lines_output([a + b])))
        elif kind == 5:
            a = i % 2
            b = (i + 1) % 2
            expected = 1 if (a == 1 or b == 1) else 0
            src = f"""fn main(): i32 {{
    let a = {a}
    let b = {b}
    if (a == 1 or b == 1) {{
        print(1)
    }} else {{
        print(0)
    }}
    return 0
}}"""
            cases.append(Case(f"reg_bool_{i:03}", src, lines_output([expected])))
        elif kind == 6:
            text = f"case-{i}"
            src = f"""fn main(): i32 {{
    print("{text}")
    return 0
}}"""
            cases.append(Case(f"reg_string_{i:03}", src, f"{text}\n"))
        elif kind == 7:
            x = i + 1
            y = i + 2
            src = f"""type Pair = struct {{
    x: i32,
    y: i32
}}

fn main(): i32 {{
    let p = Pair{{ x: {x}, y: {y} }}
    print(p.x + p.y)
    return 0
}}"""
            cases.append(Case(f"reg_struct_{i:03}", src, lines_output([x + y])))
        elif kind == 8:
            n = i % 6
            expected = 10 if n == 0 else 20 if n == 1 else 30
            src = f"""fn main(): i32 {{
    let n = {n}
    match n {{
        0 -> {{ print(10) }}
        1 -> {{ print(20) }}
        _ -> {{ print(30) }}
    }}
    return 0
}}"""
            cases.append(Case(f"reg_match_{i:03}", src, lines_output([expected])))
        else:
            a = i % 13 + 1
            b = i % 5 + 1
            expected = (a * b) - (a - b)
            src = f"""fn main(): i32 {{
    let a = {a}
    let b = {b}
    print((a * b) - (a - b))
    return 0
}}"""
            cases.append(Case(f"reg_nested_expr_{i:03}", src, lines_output([expected])))
    return cases


def fuzz_cases(count, seed=0xC0FFEE):
    rng = random.Random(seed)
    cases = []
    for i in range(count):
        values = [rng.randint(0, 50) for _ in range(rng.randint(3, 8))]
        op = rng.choice(["sum", "weighted", "branch", "loop"])
        if op == "sum":
            body = "\n".join(f"    let v{j} = {value}" for j, value in enumerate(values))
            expr = " + ".join(f"v{j}" for j in range(len(values)))
            expected = sum(values)
            src = f"""fn main(): i32 {{
{body}
    print({expr})
    return 0
}}"""
        elif op == "weighted":
            body = "\n".join(f"    let v{j} = {value}" for j, value in enumerate(values))
            terms = " + ".join(f"(v{j} * {j + 1})" for j in range(len(values)))
            expected = sum(value * (j + 1) for j, value in enumerate(values))
            src = f"""fn main(): i32 {{
{body}
    print({terms})
    return 0
}}"""
        elif op == "branch":
            x = values[0]
            y = values[1]
            expected = x - y if x > y else y - x
            src = f"""fn main(): i32 {{
    let x = {x}
    let y = {y}
    if (x > y) {{
        print(x - y)
    }} else {{
        print(y - x)
    }}
    return 0
}}"""
        else:
            hi = rng.randint(1, 25)
            expected = sum(range(hi))
            src = f"""fn main(): i32 {{
    let mut total = 0
    for (i in 0 .. {hi}) {{
        total = total + i
    }}
    print(total)
    return 0
}}"""
        cases.append(Case(f"fuzz_{i:03}_{op}", src, lines_output([expected])))
    return cases


def stress_cases(count):
    cases = []
    for i in range(count):
        kind = i % 5
        scale = i + 5
        if kind == 0:
            depth = min(scale, 24)
            expr = "1"
            for _ in range(depth):
                expr = f"({expr} + 1)"
            src = f"""fn main(): i32 {{
    print({expr})
    return 0
}}"""
            cases.append(Case(f"stress_deep_expr_{i:03}", src, lines_output([depth + 1])))
        elif kind == 1:
            count_vars = min(scale * 2, 60)
            lets = "\n".join(f"    let v{j} = {j}" for j in range(count_vars))
            expr = " + ".join(f"v{j}" for j in range(count_vars))
            cases.append(Case(
                f"stress_many_vars_{i:03}",
                f"""fn main(): i32 {{
{lets}
    print({expr})
    return 0
}}""",
                lines_output([sum(range(count_vars))]),
            ))
        elif kind == 2:
            count_fns = min(scale, 28)
            funcs = "\n".join(
                f"fn f{j}(): i32 {{\n    return {j}\n}}\n" for j in range(count_fns)
            )
            calls = " + ".join(f"f{j}()" for j in range(count_fns))
            cases.append(Case(
                f"stress_many_funcs_{i:03}",
                f"""{funcs}
fn main(): i32 {{
    print({calls})
    return 0
}}""",
                lines_output([sum(range(count_fns))]),
            ))
        elif kind == 3:
            outer = min(scale, 30)
            inner = 7
            expected = outer * inner
            src = f"""fn main(): i32 {{
    let mut total = 0
    for (i in 0 .. {outer}) {{
        for (j in 0 .. {inner}) {{
            total = total + 1
        }}
    }}
    print(total)
    return 0
}}"""
            cases.append(Case(f"stress_nested_loops_{i:03}", src, lines_output([expected])))
        else:
            depth = min(scale, 18)
            lines = ["fn main(): i32 {"]
            for j in range(depth):
                lines.append("    " * (j + 1) + "if (1) {")
            lines.append("    " * (depth + 1) + f"print({depth})")
            for j in range(depth):
                lines.append("    " * (depth - j) + "}")
            lines.append("    return 0")
            lines.append("}")
            cases.append(Case(f"stress_deep_if_{i:03}", "\n".join(lines), lines_output([depth])))
    return cases


def run_case(compiler, case, tmpdir):
    src = tmpdir / f"{case.name}.vix"
    ir = tmpdir / f"{case.name}.ll"
    src.write_text(case.source)

    compile_res = run([str(compiler), str(src)], timeout=30)
    if compile_res.returncode != 0:
        return f"{case.name}: compile failed\n{compile_res.stderr}{compile_res.stdout}"
    ir.write_text(compile_res.stdout)

    verify_res = run(["opt", "-passes=verify", "-disable-output", str(ir)], timeout=30)
    if verify_res.returncode != 0:
        return f"{case.name}: LLVM verify failed\n{verify_res.stderr}"

    run_res = run(["lli", str(ir)], timeout=10)
    if run_res.returncode != 0:
        return f"{case.name}: lli failed\n{run_res.stderr}{run_res.stdout}"
    if run_res.stdout != case.expected:
        return (
            f"{case.name}: output mismatch\n"
            f"expected: {case.expected!r}\n"
            f"actual:   {run_res.stdout!r}"
        )
    return None


def parse_args(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("--regression", type=int, default=100)
    parser.add_argument("--fuzz", type=int, default=300)
    parser.add_argument("--stress", type=int, default=20)
    parser.add_argument("--compiler", action="append", type=Path)
    parser.add_argument("--fail-fast", action="store_true")
    return parser.parse_args(argv)


def main(argv=None):
    args = parse_args(argv or sys.argv[1:])
    compilers = tuple(args.compiler) if args.compiler else DEFAULT_COMPILERS
    missing = [str(path) for path in compilers if not path.exists()]
    if missing:
        print("missing compiler(s): " + ", ".join(missing), file=sys.stderr)
        return 1

    suites = [
        ("regression", regression_cases(args.regression)),
        ("fuzz", fuzz_cases(args.fuzz)),
        ("stress", stress_cases(args.stress)),
    ]
    failures = []
    with tempfile.TemporaryDirectory(prefix="vix-bootstrap-stability-") as raw_tmp:
        root_tmp = Path(raw_tmp)
        for compiler in compilers:
            compiler_tmp = root_tmp / compiler.name
            compiler_tmp.mkdir()
            for suite_name, cases in suites:
                for case in cases:
                    failure = run_case(compiler, case, compiler_tmp)
                    if failure:
                        failures.append(f"{compiler.name}/{suite_name}/{failure}")
                        if args.fail_fast:
                            print(failures[-1], file=sys.stderr)
                            return 1
                print(f"PASS {compiler.name} {suite_name} {len(cases)}")

    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    total = sum(len(cases) for _, cases in suites)
    print(f"PASS all {len(compilers)} compiler(s), {total} cases each")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
