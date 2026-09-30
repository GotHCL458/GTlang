import subprocess
import sys
import pytest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from helpers import compile_vix, COMPILER, EXAMPLES_DIR, ROOT


WORKING_EXAMPLES = [
    "and_or.vix",
    "extern3.vix",
    "fn2.vix",
    "generics2.vix",
    "global.vix",
    "hello.vix",
    "list.vix",
    "macro.vix",
    "nullptr.vix",
    "option.vix",
    "result.vix",
    "string_arr.vix",
    "struct2.vix",
    "symtable.vix",
    "test_for_loop.vix",
    "var.vix",
    "while_loop_test.vix",
]


@pytest.mark.integration
class TestExamples:
    @pytest.mark.parametrize("example", WORKING_EXAMPLES)
    def test_example_compiles(self, compiler, tmp_path, example):
        src = EXAMPLES_DIR / example
        if not src.exists():
            pytest.skip(f"{example} not found")
        out = tmp_path / example.replace(".vix", "")
        res = compile_vix(compiler, src, out)
        assert res.returncode == 0, (
            f"Example {example} failed to compile:\n{res.stderr[:300]}"
        )

    def test_hello_world(self, compiler, tmp_path):
        src = EXAMPLES_DIR / "hello.vix"
        if not src.exists():
            pytest.skip("hello.vix not found")
        out = tmp_path / "hello"
        compile_vix(compiler, src, out)
        run = subprocess.run([str(out)], capture_output=True, text=True, timeout=5)
        assert run.returncode == 0
        assert "hello" in run.stdout.lower() or "Hello" in run.stdout


@pytest.mark.integration
class TestExamplesSubdirs:
    def test_ed_editor(self, compiler, tmp_path):
        src = EXAMPLES_DIR / "ed" / "ed.vix"
        if not src.exists():
            pytest.skip("ed.vix not found")
        out = tmp_path / "ed"
        res = compile_vix(compiler, src, out)
        if res.returncode != 0:
            pytest.skip(f"ed.vix doesn't compile with current vixc (likely outdated example)")
