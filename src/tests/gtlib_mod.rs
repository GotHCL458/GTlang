//! 标准库模块表的单元测试（三处同步的守护）。
#![cfg(test)]

use super::{dll_of, MODULES};

#[test]
fn every_func_maps_to_its_dll() {
    for m in MODULES {
        for f in m.funcs {
            assert_eq!(dll_of(f), Some(m.dll), "func {} should map to {}", f, m.dll);
        }
    }
}

#[test]
fn unknown_func_returns_none() {
    assert_eq!(dll_of("绝对不存在的函数名"), None);
    assert_eq!(dll_of(""), None);
}

#[test]
fn known_funcs_resolve() {
    assert_eq!(dll_of("sqrt"), Some("math"));
    assert_eq!(dll_of("gcd"), Some("math"));
    assert_eq!(dll_of("strip"), Some("string"));
    assert_eq!(dll_of("json_dumps"), Some("json"));
    assert_eq!(dll_of("getcwd"), Some("os"));
    assert_eq!(dll_of("toml_loads"), Some("toml"));
    assert_eq!(dll_of("tcp_connect"), Some("net"));
}

#[test]
fn dll_names_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for m in MODULES {
        assert!(seen.insert(m.dll), "duplicate dll name: {}", m.dll);
    }
}

#[test]
fn funcs_within_module_are_unique() {
    for m in MODULES {
        let mut seen = std::collections::HashSet::new();
        for f in m.funcs {
            assert!(seen.insert(*f), "duplicate func {} in module {}", f, m.dll);
        }
    }
}

#[test]
fn modules_are_non_empty() {
    assert!(!MODULES.is_empty());
    for m in MODULES {
        assert!(!m.dll.is_empty());
        assert!(!m.funcs.is_empty(), "module {} has no funcs", m.dll);
    }
}
