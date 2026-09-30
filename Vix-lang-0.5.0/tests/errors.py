import sys
import pytest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from helpers import compile_vix, compile_and_run


@pytest.mark.error
class TestTypeErrors:
    def test_type_mismatch_function_return(self, compiler, tmp_path):
        src = 'fn foo(): i32 { return "hello" } fn main(): i32 { return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "expected type" in res.stderr.lower() or "type mismatch" in res.stderr.lower()

    def test_type_mismatch_binary_op(self, compiler, tmp_path):
        src = 'fn main(): i32 { let x = "hello" + 5 return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "type" in res.stderr.lower()

    def test_type_mismatch_return_void(self, compiler, tmp_path):
        src = 'fn main() { return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "expected type" in res.stderr.lower() or "type mismatch" in res.stderr.lower()


@pytest.mark.error
class TestUndefinedIdentifiers:
    def test_undefined_variable(self, compiler, tmp_path):
        src = 'fn main(): i32 { print(undefined_var) return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "undefined" in res.stderr.lower() or "not found" in res.stderr.lower()

    def test_undefined_function(self, compiler, tmp_path):
        src = 'fn main(): i32 { print(undefined_fn()) return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0


@pytest.mark.error
class TestSelfRecursiveStruct:
    def test_self_recursive_struct(self, compiler, tmp_path):
        src = '''struct Node {
            value: i32,
            next: Node
        }
        fn main(): i32 { return 0 }'''
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "recursive" in res.stderr.lower() or "self-recursive" in res.stderr.lower()


@pytest.mark.error
class TestCapturingLocals:
    def test_capturing_local_variable(self, compiler, tmp_path):
        src = '''fn main(): i32 {
            let x = 5
            fn inner(): i32 { return x }
            print(inner())
            return 0
        }'''
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "capturing" in res.stderr.lower()


@pytest.mark.error
class TestMatchExhaustiveness:
    def test_non_exhaustive_match_adt(self, compiler, tmp_path):
        src = '''type Status = Active | Inactive
        fn main(): i32 {
            let s = Active
            match s {
                Active -> { print("active") }
            }
            return 0
        }'''
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "exhaustive" in res.stderr.lower() or "non-exhaustive" in res.stderr.lower()


@pytest.mark.error
class TestRedefinition:
    def test_redefinition_variable(self, compiler, tmp_path):
        src = '''fn main(): i32 {
            let x = 10
            let x = 20
            return 0
        }'''
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "redefinition" in res.stderr.lower() or "redefin" in res.stderr.lower()


@pytest.mark.error
class TestDiagnosticsQuality:
    def test_arrow_return_type_is_error(self, compiler, tmp_path):
        src = 'fn foo() -> i32 { return 1 } fn main(): i32 { return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "invalid function return syntax" in res.stderr.lower()

    def test_syntax_error_reports_location(self, compiler, tmp_path):
        src = '''fn main(): i32 {
            let x = @@@
            return 0
        }'''
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0

    def test_type_error_has_location_info(self, compiler, tmp_path):
        src = 'fn foo(): i32 { return "hello" } fn main(): i32 { return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "-->" in res.stderr


@pytest.mark.error
class TestErrorCodes:
    def test_error_code_in_output(self, compiler, tmp_path):
        src = 'fn foo(): i32 { return "hello" } fn main(): i32 { return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "E00" in res.stderr

    def test_semantic_error_has_e006(self, compiler, tmp_path):
        src = 'fn main(): i32 { let x = [1, 2]; let y = x; print(x[0]); return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "E006" in res.stderr

    def test_type_error_has_e003(self, compiler, tmp_path):
        src = 'fn foo(): i32 { return "hello" } fn main(): i32 { return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "E003" in res.stderr


@pytest.mark.error
class TestUseAfterMove:
    def test_use_after_move_has_note(self, compiler, tmp_path):
        src = '''fn main(): i32 {
    let buf = [1, 2, 3]
    let t = buf
    print(buf[0])
    return 0
}'''
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "note:" in res.stderr.lower() or "was moved" in res.stderr.lower()

    def test_use_after_move_shows_location(self, compiler, tmp_path):
        src = '''fn main(): i32 {
    let buf = [1, 2, 3]
    let t = buf
    print(buf[0])
    return 0
}'''
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        # Should show the line where the move happened
        assert "let t = buf" in res.stderr or "moved at" in res.stderr


@pytest.mark.error
class TestOwnershipErrors:
    def test_use_after_move_reported(self, compiler, tmp_path):
        src = 'fn main(): i32 { let x = [1, 2]; let y = x; print(x[0]); return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "use of moved value" in res.stderr

    def test_mut_borrow_twice(self, compiler, tmp_path):
        src = 'fn main(): i32 { let mut x = 10; let r1 = mut ref x; let r2 = mut ref x; print(@r1); return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrow" in res.stderr.lower()
class TestEdgeCaseErrors:
    def test_empty_source(self, compiler, tmp_path):
        src = ''
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0

    def test_only_comments(self, compiler, tmp_path):
        src = '// just a comment\n/* block */'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0

    def test_missing_closing_brace(self, compiler, tmp_path):
        src = 'fn main(): i32 { print("hello") return 0'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0

    def test_missing_return_type(self, compiler, tmp_path):
        src = 'fn main() { return 0 }'
        res, _ = compile_and_run(compiler, src, tmp_path)
        assert res.returncode != 0
