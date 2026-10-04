//! 整数范围分析的单元测试。
#![cfg(test)]

use super::Range;

#[test]
fn unknown_and_exact() {
    let u = Range::unknown();
    assert!(!u.is_known());
    assert_eq!(u.lo, i64::MIN);
    assert_eq!(u.hi, i64::MAX);
    let e = Range::exact(5);
    assert!(e.is_known());
    assert_eq!(e.lo, 5);
    assert_eq!(e.hi, 5);
}

#[test]
fn add_known_ranges() {
    let a = Range { lo: 0, hi: 10 };
    let b = Range { lo: 5, hi: 7 };
    let r = a.add(&b);
    assert_eq!(r.lo, 5);
    assert_eq!(r.hi, 17);
}

#[test]
fn add_overflow_becomes_unknown() {
    let a = Range { lo: i64::MAX - 1, hi: i64::MAX };
    let b = Range::exact(5);
    let r = a.add(&b);
    assert!(!r.is_known(), "overflow must yield unknown");
}

#[test]
fn sub_known_ranges() {
    // [10,20] - [1,3] = [7,19]
    let a = Range { lo: 10, hi: 20 };
    let b = Range { lo: 1, hi: 3 };
    let r = a.sub(&b);
    assert_eq!(r.lo, 7);
    assert_eq!(r.hi, 19);
}

#[test]
fn mul_known_ranges() {
    // [-2,3] * [4,5] = [-10, 15]
    let a = Range { lo: -2, hi: 3 };
    let b = Range { lo: 4, hi: 5 };
    let r = a.mul(&b);
    assert_eq!(r.lo, -10);
    assert_eq!(r.hi, 15);
}

#[test]
fn mul_overflow_becomes_unknown() {
    let a = Range { lo: i64::MAX / 2, hi: i64::MAX };
    let b = Range::exact(4);
    let r = a.mul(&b);
    assert!(!r.is_known(), "mul overflow must yield unknown");
}

#[test]
fn join_takes_union() {
    let a = Range { lo: 1, hi: 5 };
    let b = Range { lo: 3, hi: 10 };
    let r = a.join(&b);
    assert_eq!(r.lo, 1);
    assert_eq!(r.hi, 10);
}

#[test]
fn exact_is_known() {
    for v in [0i64, 1, -1, i64::MAX, i64::MIN] {
        let r = Range::exact(v);
        assert!(r.is_known(), "exact({}) should be known", v);
        assert_eq!(r.lo, v);
        assert_eq!(r.hi, v);
    }
}
