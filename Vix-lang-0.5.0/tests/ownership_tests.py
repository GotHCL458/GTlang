"""
Comprehensive Ownership & Borrow Checker Tests

Tests the ownership checker (field-level borrows, NLL, mut ptr, move semantics)
by compiling .vix files and checking for expected pass/fail outcomes.

Each test compiles a small Vix source with --check and asserts:
  - Good tests:  returncode == 0 (no errors)
  - Error tests: returncode != 0 (ownership violation detected)
"""

import pytest
from pathlib import Path
from helpers import compile_source, ROOT

# ──────────────────────────────────────────────
#  FIELD-LEVEL BORROW PRECISION
# ──────────────────────────────────────────────

@pytest.mark.ownership
class TestFieldLevelBorrows:

    def test_shared_borrow_field_other_readable(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let p = Point { x: 10, y: 20 }
    let r = ref p.x
    print(p.y)
    @r
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0, f"stderr: {res.stderr}"

    def test_mut_borrow_field_other_writable(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let mut p = Point { x: 10, y: 20 }
    let r = mut ref p.x
    p.y = 30
    print(@r)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0, f"stderr: {res.stderr}"

    def test_two_shared_borrows_diff_fields(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let p = Point { x: 10, y: 20 }
    let rx = ref p.x
    let ry = ref p.y
    print(@rx + @ry)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0

    def test_use_mut_borrowed_field_rejected(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let mut p = Point { x: 10, y: 20 }
    let r = mut ref p.x
    print(p.x)
    @r
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrowed" in res.stderr

    def test_assign_to_mut_borrowed_field_rejected(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let mut p = Point { x: 10, y: 20 }
    let r = mut ref p.x
    p.x = 30
    @r
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrowed" in res.stderr

    def test_assign_to_shared_borrowed_field_rejected(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let mut p = Point { x: 10, y: 20 }
    let r = ref p.x
    p.x = 30
    @r
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrowed" in res.stderr

    def test_move_while_field_borrowed_rejected(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let p = Point { x: 10, y: 20 }
    let r = ref p.x
    let q = p
    print(@r)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrowed" in res.stderr


# ──────────────────────────────────────────────
#  NLL (NON-LEXICAL LIFETIMES)
# ──────────────────────────────────────────────

@pytest.mark.ownership
class TestNLL:

    def test_nll_shared_read_after_last_use(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let x = 42
    let r = ref x
    print(@r)
    print(x)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0, f"stderr: {res.stderr}"

    def test_nll_mut_assign_after_last_use(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut x = 10
    let r = mut ref x
    print(@r)
    x = 30
    print(x)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0, f"stderr: {res.stderr}"

    def test_nll_field_use_after_last_use(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let mut p = Point { x: 1, y: 2 }
    let r = ref p.x
    print(@r)
    p.x = 42
    print(p.x + p.y)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0

    def test_nll_two_borrowers_one_dies_early(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut x = 10
    let r1 = ref x
    print(@r1)
    x = 20
    let r2 = ref x
    print(@r2)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0

    def test_use_while_borrow_still_alive_rejected(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut x = 42
    let r = mut ref x
    print(x)
    print(@r)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrowed" in res.stderr


# ──────────────────────────────────────────────
#  MUTABLE BORROW + POINTER WRITE
# ──────────────────────────────────────────────

@pytest.mark.ownership
class TestMutablePtrWrite:

    def test_mut_ptr_write_simple(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut x = 10
    let mut ptr = mut ref x
    @ptr = 20
    return @ptr
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0, f"stderr: {res.stderr}"

    def test_mut_ptr_write_then_reborrow(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut x = 10
    let mut ptr = mut ref x
    @ptr = 99
    print(@ptr)
    x = 100
    print(x)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0

    def test_write_through_shared_borrow_rejected(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut x = 10
    let ptr = ref x
    let mut ptr2 = ptr
    @ptr2 = 20
    return @ptr
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "immutable pointer" in res.stderr or "borrowed" in res.stderr


# ──────────────────────────────────────────────
#  MOVE SEMANTICS
# ──────────────────────────────────────────────

@pytest.mark.ownership
class TestMoveSemantics:

    def test_copy_type_not_moved(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let x = 10
    let y = x
    print(x + y)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0

    def test_use_after_move_rejected(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let s = [1, 2, 3]
    let t = s
    print(s[0])
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "use of moved value" in res.stderr

    def test_use_after_move_call_rejected(self, compiler, tmp_path):
        src = """
fn take(xs: [i32]): i32 { xs[0] }
fn main(): i32 {
    let arr = [1, 2, 3]
    take(arr)
    print(arr[0])
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "use of moved value" in res.stderr


# ──────────────────────────────────────────────
#  BORROW CONFLICTS
# ──────────────────────────────────────────────

@pytest.mark.ownership
class TestBorrowConflicts:

    def test_mut_borrow_twice_rejected(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut v = 10
    let r1 = mut ref v
    let r2 = mut ref v
    print(@r1)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrow" in res.stderr.lower()

    def test_imm_borrow_while_mut_rejected(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut y = 99
    let ry = mut ref y
    let ry2 = ref y
    print(@ry)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrow" in res.stderr.lower()

    def test_assign_while_mut_borrowed_rejected(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut x = 10
    let r = mut ref x
    x = 20
    print(@r)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrowed" in res.stderr.lower()

    def test_move_while_borrowed_rejected(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let s = [1, 2, 3]
    let r = ref s[0]
    let t = s
    print(@r)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "cannot move" in res.stderr

    def test_borrow_moved_value_rejected(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let s = [1, 2, 3]
    let t = s
    let r = ref s
    print(@r)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "moved" in res.stderr

    def test_return_local_ref_rejected(self, compiler, tmp_path):
        src = """
fn bad(): ref i32 { let z = 1; return ref z }
fn main(): i32 { print(bad()); return 0 }"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "return reference" in res.stderr


# ──────────────────────────────────────────────
#  IF/ELSE BRANCH MERGE
# ──────────────────────────────────────────────

@pytest.mark.ownership
class TestIfElseBranchMerge:

    def test_field_borrow_merge_if_else(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let mut p = Point { x: 1, y: 2 }
    let cond = 1 < 2
    if (cond) {
        let r = ref p.x
        print(@r)
    } else {
        let r = ref p.y
        print(@r)
    }
    p.x = 10
    p.y = 20
    print(p.x + p.y)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0, f"stderr: {res.stderr}"

    def test_nll_after_if_else(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut x = 10
    let r = ref x
    print(@r)
    if (1 < 2) {
        x = 20
    } else {
        x = 30
    }
    print(x)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0


# ──────────────────────────────────────────────
#  WHILE LOOP
# ──────────────────────────────────────────────

@pytest.mark.ownership
class TestWhileLoop:

    def test_while_loop_inner_borrow(self, compiler, tmp_path):
        src = """
fn main(): i32 {
    let mut sum = 0
    let mut i = 0
    while (i < 3) {
        let r = ref sum
        print(@r)
        i = i + 1
    }
    print(sum)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode == 0


# ──────────────────────────────────────────────
#  OVERLAPPING FIELD BORROWS
# ──────────────────────────────────────────────

@pytest.mark.ownership
class TestOverlappingFieldBorrows:

    def test_overlapping_field_mut_rejected(self, compiler, tmp_path):
        src = """
type Point = struct { x: i32, y: i32 }
fn main(): i32 {
    let mut p = Point { x: 1, y: 2 }
    let rx = mut ref p.x
    let rx2 = mut ref p.x
    print(@rx)
    return 0
}"""
        res, _ = compile_source(compiler, src, tmp_path)
        assert res.returncode != 0
        assert "borrow" in res.stderr.lower()
