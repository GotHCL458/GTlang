//! Hindley-Milner 类型合一（移植自 Vix-lang 的 include/unify.h，改写为操作 GTLang 的 Ty）。
//!
//! 提供：
//!   - 类型变量分配（fresh）
//!   - 解引用/应用（apply，把已解出的变量替换掉）
//!   - 合一（unify，含 occurs check 与事务回滚）
//!
//! 只在"未标注参数/泛型函数体"的推断路径上启用；既有 sema 逻辑不受影响。

use std::collections::HashMap;
use crate::types::Ty;

pub struct Unifier {
    next: u32,
    subst: HashMap<u32, Ty>,
    /// 事务栈（unify 失败可回滚）
    undo: Vec<HashMap<u32, Ty>>,
}

impl Unifier {
    pub fn new() -> Self {
        Unifier { next: 0, subst: HashMap::new(), undo: Vec::new() }
    }

    /// 分配一个新的类型变量
    pub fn fresh(&mut self) -> Ty {
        let id = self.next;
        self.next += 1;
        Ty::Var(id)
    }

    fn resolve(&self, id: u32) -> Option<&Ty> {
        self.subst.get(&id)
    }

    /// 把类型里已解出的变量递归替换（对应 unify.h 的 apply）
    pub fn apply(&self, t: &Ty) -> Ty {
        match t {
            Ty::Var(id) => match self.resolve(*id) {
                Some(inner) => self.apply(&inner.clone()),
                None => Ty::Var(*id),
            },
            Ty::Array(e, n) => Ty::Array(Box::new(self.apply(e)), *n),
            Ty::List(e) => Ty::List(Box::new(self.apply(e))),
            Ty::Set(e) => Ty::Set(Box::new(self.apply(e))),
            Ty::Map(k, v) => Ty::Map(Box::new(self.apply(k)), Box::new(self.apply(v))),
            Ty::Closure(ps, r) => Ty::Closure(ps.iter().map(|p| self.apply(p)).collect(), Box::new(self.apply(r))),
            Ty::Result(a, b) => Ty::Result(Box::new(self.apply(a)), Box::new(self.apply(b))),
            Ty::Option(a) => Ty::Option(Box::new(self.apply(a))),
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|x| self.apply(x)).collect()),
            Ty::Ref(a) => Ty::Ref(Box::new(self.apply(a))),
            Ty::RefMut(a) => Ty::RefMut(Box::new(self.apply(a))),
            other => other.clone(),
        }
    }

    fn begin(&mut self) { self.undo.push(self.subst.clone()); }
    fn commit(&mut self) { self.undo.pop(); }
    fn rollback(&mut self) { if let Some(s) = self.undo.pop() { self.subst = s; } }

    /// 合一（带事务：失败回滚）
    pub fn unify(&mut self, a: &Ty, b: &Ty) -> Result<(), String> {
        self.begin();
        match self.unify_impl(a, b) {
            Ok(()) => { self.commit(); Ok(()) }
            Err(e) => { self.rollback(); Err(e) }
        }
    }

    fn unify_impl(&mut self, a: &Ty, b: &Ty) -> Result<(), String> {
        let a = self.apply(a);
        let b = self.apply(b);
        if a == b { return Ok(()); }
        match (&a, &b) {
            // 类型变量：绑定
            (Ty::Var(id), _) => { self.bind(*id, &b) }
            (_, Ty::Var(id)) => { self.bind(*id, &a) }
            // 结构相同则递归
            (Ty::Array(x, n), Ty::Array(y, m)) if n == m => self.unify_impl(x, y),
            (Ty::List(x), Ty::List(y)) => self.unify_impl(x, y),
            (Ty::Set(x), Ty::Set(y)) => self.unify_impl(x, y),
            (Ty::Map(k1, v1), Ty::Map(k2, v2)) => { self.unify_impl(k1, k2)?; self.unify_impl(v1, v2) }
            (Ty::Result(a1, b1), Ty::Result(a2, b2)) => { self.unify_impl(a1, a2)?; self.unify_impl(b1, b2) }
            (Ty::Option(x), Ty::Option(y)) => self.unify_impl(x, y),
            (Ty::Ref(x), Ty::Ref(y)) => self.unify_impl(x, y),
            (Ty::RefMut(x), Ty::RefMut(y)) => self.unify_impl(x, y),
            (Ty::Tuple(xs), Ty::Tuple(ys)) if xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys.iter()) { self.unify_impl(x, y)?; }
                Ok(())
            }
            (Ty::Closure(p1, r1), Ty::Closure(p2, r2)) if p1.len() == p2.len() => {
                for (x, y) in p1.iter().zip(p2.iter()) { self.unify_impl(x, y)?; }
                self.unify_impl(r1, r2)
            }
            // Unknown 与任何类型合一（宽松）
            (Ty::Unknown, _) | (_, Ty::Unknown) => Ok(()),
            _ => Err(format!("cannot unify {} with {}", a, b)),
        }
    }

    fn occurs(&self, id: u32, t: &Ty) -> bool {
        match t {
            Ty::Var(x) => {
                if *x == id { return true; }
                match self.resolve(*x) { Some(inner) => self.occurs(id, &inner.clone()), None => false }
            }
            Ty::Array(e, _) | Ty::List(e) | Ty::Set(e) | Ty::Option(e) | Ty::Ref(e) | Ty::RefMut(e) => self.occurs(id, e),
            Ty::Map(k, v) | Ty::Result(k, v) => self.occurs(id, k) || self.occurs(id, v),
            Ty::Tuple(ts) | Ty::Closure(ts, _) => ts.iter().any(|x| self.occurs(id, x)),
            _ => false,
        }
    }

    fn bind(&mut self, id: u32, t: &Ty) -> Result<(), String> {
        if let Ty::Var(other) = t {
            if *other == id { return Ok(()); }
        }
        if self.occurs(id, t) {
            return Err(format!("recursive type: {} occurs in {}", Ty::Var(id), t));
        }
        self.subst.insert(id, t.clone());
        Ok(())
    }
}

impl Default for Unifier { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Ty;

    #[test]
    fn fresh_is_distinct() {
        let mut u = Unifier::new();
        let a = u.fresh();
        let b = u.fresh();
        assert_ne!(a, b);
    }

    #[test]
    fn unify_var_with_concrete() {
        let mut u = Unifier::new();
        let v = u.fresh();
        assert!(u.unify(&v, &Ty::I64).is_ok());
        assert_eq!(u.apply(&v), Ty::I64);
    }

    #[test]
    fn unify_mismatch_fails() {
        let mut u = Unifier::new();
        assert!(u.unify(&Ty::I64, &Ty::Str).is_err());
    }

    #[test]
    fn unify_list_element() {
        let mut u = Unifier::new();
        let e = u.fresh();
        let la = Ty::List(Box::new(e.clone()));
        let lb = Ty::List(Box::new(Ty::I64));
        assert!(u.unify(&la, &lb).is_ok());
        assert_eq!(u.apply(&e), Ty::I64);
    }

    #[test]
    fn occurs_check() {
        let mut u = Unifier::new();
        let v = u.fresh();
        let t = Ty::List(Box::new(v.clone()));
        assert!(u.unify(&v, &t).is_err(), "occurs check should fail");
    }

    #[test]
    fn rollback_on_failure() {
        let mut u = Unifier::new();
        let v = u.fresh();
        // v 先与 I64 绑定，再与 Str 合一 -> 失败，且不应污染 subst
        assert!(u.unify(&v, &Ty::I64).is_ok());
        assert!(u.unify(&Ty::List(Box::new(v.clone())), &Ty::List(Box::new(Ty::Str))).is_err());
        assert_eq!(u.apply(&v), Ty::I64, "failed unify must not mutate bindings");
    }

    #[test]
    fn apply_is_transitive() {
        let mut u = Unifier::new();
        let a = u.fresh();
        let b = u.fresh();
        assert!(u.unify(&a, &b).is_ok());
        assert!(u.unify(&b, &Ty::I64).is_ok());
        assert_eq!(u.apply(&a), Ty::I64);
        assert_eq!(u.apply(&b), Ty::I64);
    }

    #[test]
    fn unify_map_key_and_value() {
        let mut u = Unifier::new();
        let k = u.fresh();
        let v = u.fresh();
        let a = Ty::Map(Box::new(k.clone()), Box::new(v.clone()));
        let b = Ty::Map(Box::new(Ty::Str), Box::new(Ty::F64));
        assert!(u.unify(&a, &b).is_ok());
        assert_eq!(u.apply(&k), Ty::Str);
        assert_eq!(u.apply(&v), Ty::F64);
    }

    #[test]
    fn unify_closure_signature() {
        let mut u = Unifier::new();
        let p = u.fresh();
        let r = u.fresh();
        let a = Ty::Closure(vec![p.clone()], Box::new(r.clone()));
        let b = Ty::Closure(vec![Ty::I64], Box::new(Ty::Str));
        assert!(u.unify(&a, &b).is_ok());
        assert_eq!(u.apply(&p), Ty::I64);
        assert_eq!(u.apply(&r), Ty::Str);
    }

    #[test]
    fn unify_result_and_option() {
        let mut u = Unifier::new();
        let a = u.fresh();
        let b = u.fresh();
        let x = Ty::Result(Box::new(a.clone()), Box::new(b.clone()));
        let y = Ty::Result(Box::new(Ty::I64), Box::new(Ty::Str));
        assert!(u.unify(&x, &y).is_ok());
        assert_eq!(u.apply(&a), Ty::I64);
        assert_eq!(u.apply(&b), Ty::Str);

        let c = u.fresh();
        assert!(u.unify(&Ty::Option(Box::new(c.clone())), &Ty::Option(Box::new(Ty::F64))).is_ok());
        assert_eq!(u.apply(&c), Ty::F64);
    }

    #[test]
    fn unify_tuple_and_ref() {
        let mut u = Unifier::new();
        let a = u.fresh();
        let b = u.fresh();
        let ta = Ty::Tuple(vec![a.clone(), b.clone()]);
        let tb = Ty::Tuple(vec![Ty::I64, Ty::Str]);
        assert!(u.unify(&ta, &tb).is_ok());
        assert_eq!(u.apply(&a), Ty::I64);
        assert_eq!(u.apply(&b), Ty::Str);

        let r = u.fresh();
        assert!(u.unify(&Ty::Ref(Box::new(r.clone())), &Ty::Ref(Box::new(Ty::F64))).is_ok());
        assert_eq!(u.apply(&r), Ty::F64);
    }

    #[test]
    fn unify_same_var_twice() {
        let mut u = Unifier::new();
        let v = u.fresh();
        assert!(u.unify(&v, &Ty::I64).is_ok());
        // 同一变量再次与相同类型合一：成功
        assert!(u.unify(&v, &Ty::I64).is_ok());
        // 与不同类型合一：失败
        assert!(u.unify(&v, &Ty::Str).is_err());
        assert_eq!(u.apply(&v), Ty::I64);
    }

    #[test]
    fn array_and_set_unify() {
        let mut u = Unifier::new();
        let e = u.fresh();
        assert!(u.unify(&Ty::Array(Box::new(e.clone()), 3), &Ty::Array(Box::new(Ty::I64), 3)).is_ok());
        assert_eq!(u.apply(&e), Ty::I64);
        // 长度不同 → 失败
        assert!(u.unify(&Ty::Array(Box::new(Ty::I64), 3), &Ty::Array(Box::new(Ty::I64), 4)).is_err());

        let s = u.fresh();
        assert!(u.unify(&Ty::Set(Box::new(s.clone())), &Ty::Set(Box::new(Ty::Str))).is_ok());
        assert_eq!(u.apply(&s), Ty::Str);
    }

    #[test]
    fn unknown_unifies_with_anything() {
        let mut u = Unifier::new();
        assert!(u.unify(&Ty::Unknown, &Ty::I64).is_ok());
        assert!(u.unify(&Ty::Str, &Ty::Unknown).is_ok());
    }
}
