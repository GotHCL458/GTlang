import pytest

from helpers import compile_source


@pytest.mark.feature
class TestOwnerSystem:
    def test_owner_happy_path_is_complete(self, compiler, tmp_path):
        src = '''fn read2(a: ref i32, b: ref i32): i32 {
    return @a + @b
}

fn inc(mut p: ref i32) {
    @p = @p + 1
}

fn take_array(xs: [i32]): i32 {
    return xs[0] + xs[1]
}

fn maybe_ref(flag: i32, value: ref i32): ?ref i32 {
    if (flag == 0) { return None }
    return Some(value)
}

fn main(): i32 {
    let x = 7
    let y = x
    print(read2(ref x, ref y))

    let shared = 3
    print(read2(ref shared, ref shared))

    let mut counter = 41
    inc(mut ref counter)
    print(counter)

    let nums = [5, 6]
    let moved_nums = nums
    print(take_array(moved_nums))

    let r = maybe_ref(1, ref counter)
    match r {
        Some(ptr) -> print(@ptr)
        None -> print(0)
    }

    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode == 0, compile_res.stderr

    def test_move_consumes_non_copy_array(self, compiler, tmp_path):
        src = '''fn main(): i32 {
    let xs = [1, 2, 3]
    let ys = xs
    print(ys[0])
    print(xs[0])
    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode != 0
        assert "moved value 'xs'" in compile_res.stderr
        assert "unknown" not in compile_res.stderr

    def test_last_use_move_is_allowed(self, compiler, tmp_path):
        src = '''fn main(): i32 {
    let xs = [1, 2, 3]
    let ys = xs
    print(ys[0])
    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode == 0, compile_res.stderr

    def test_copy_values_remain_usable_after_assignment(self, compiler, tmp_path):
        src = '''fn main(): i32 {
    let x = 42
    let y = x
    print(x)
    print(y)
    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode == 0, compile_res.stderr

    def test_immutable_refs_can_be_shared_even_for_mut_variable(self, compiler, tmp_path):
        src = '''fn read2(a: ref i32, b: ref i32): i32 {
    return @a + @b
}

fn main(): i32 {
    let mut x = 1
    print(read2(ref x, ref x))
    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode == 0, compile_res.stderr

    def test_mut_ref_is_exclusive_within_statement(self, compiler, tmp_path):
        src = '''fn read2(a: ref i32, b: ref i32): i32 {
    return @a + @b
}

fn main(): i32 {
    let mut x = 1
    print(read2(mut ref x, mut ref x))
    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode != 0
        assert "mutably borrow 'x'" in compile_res.stderr

    def test_mut_ref_conflicts_with_shared_ref_in_same_statement(self, compiler, tmp_path):
        src = '''fn read2(a: ref i32, b: ref i32): i32 {
    return @a + @b
}

fn main(): i32 {
    let mut x = 1
    print(read2(mut ref x, ref x))
    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode != 0
        assert "immutably borrow 'x' while it is mutably borrowed" in compile_res.stderr

    def test_cannot_return_reference_to_local(self, compiler, tmp_path):
        src = '''fn bad(): ref i32 {
    let x = 1
    return ref x
}

fn main(): i32 {
    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode != 0
        assert "reference to local variable 'x'" in compile_res.stderr
        assert "unknown" not in compile_res.stderr

    def test_mut_ref_borrow_ends_after_statement(self, compiler, tmp_path):
        src = '''fn inc(mut p: ref i32) {
    @p = @p + 1
}

fn main(): i32 {
    let mut x = 0
    inc(mut ref x)
    inc(mut ref x)
    print(x)
    return 0
}'''
        compile_res, _ = compile_source(compiler, src, tmp_path)
        assert compile_res.returncode == 0, compile_res.stderr
