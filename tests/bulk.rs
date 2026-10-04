//! Bulk tests: 500 source snippets, frontend-only.

use gtc_rust::build;

fn ok(src: &str) {
    if let Err(diags) = build("t.gt", src) {
        let msg: Vec<String> = diags.iter().map(|d| d.message.clone()).collect();
        panic!("should pass: {} --- {}", msg.join("|"), src);
    }
}

#[test] fn t1() { ok("fn main() { a := 0 + 1 put(a) }"); }
#[test] fn t2() { ok("fn main() { a := 1 + 1 put(a) }"); }
#[test] fn t3() { ok("fn main() { a := 2 + 1 put(a) }"); }
#[test] fn t4() { ok("fn main() { a := 3 + 1 put(a) }"); }
#[test] fn t5() { ok("fn main() { a := 4 + 1 put(a) }"); }
#[test] fn t6() { ok("fn main() { a := 5 + 1 put(a) }"); }
#[test] fn t7() { ok("fn main() { a := 6 + 1 put(a) }"); }
#[test] fn t8() { ok("fn main() { a := 7 + 1 put(a) }"); }
#[test] fn t9() { ok("fn main() { a := 8 + 1 put(a) }"); }
#[test] fn t10() { ok("fn main() { a := 9 + 1 put(a) }"); }
#[test] fn t11() { ok("fn main() { a := 10 + 1 put(a) }"); }
#[test] fn t12() { ok("fn main() { a := 11 + 1 put(a) }"); }
#[test] fn t13() { ok("fn main() { a := 12 + 1 put(a) }"); }
#[test] fn t14() { ok("fn main() { a := 13 + 1 put(a) }"); }
#[test] fn t15() { ok("fn main() { a := 14 + 1 put(a) }"); }
#[test] fn t16() { ok("fn main() { a := 15 + 1 put(a) }"); }
#[test] fn t17() { ok("fn main() { a := 16 + 1 put(a) }"); }
#[test] fn t18() { ok("fn main() { a := 17 + 1 put(a) }"); }
#[test] fn t19() { ok("fn main() { a := 18 + 1 put(a) }"); }
#[test] fn t20() { ok("fn main() { a := 19 + 1 put(a) }"); }
#[test] fn t21() { ok("fn main() { a := 20 + 1 put(a) }"); }
#[test] fn t22() { ok("fn main() { a := 21 + 1 put(a) }"); }
#[test] fn t23() { ok("fn main() { a := 22 + 1 put(a) }"); }
#[test] fn t24() { ok("fn main() { a := 23 + 1 put(a) }"); }
#[test] fn t25() { ok("fn main() { a := 24 + 1 put(a) }"); }
#[test] fn t26() { ok("fn main() { a := 25 + 1 put(a) }"); }
#[test] fn t27() { ok("fn main() { a := 26 + 1 put(a) }"); }
#[test] fn t28() { ok("fn main() { a := 27 + 1 put(a) }"); }
#[test] fn t29() { ok("fn main() { a := 28 + 1 put(a) }"); }
#[test] fn t30() { ok("fn main() { a := 29 + 1 put(a) }"); }
#[test] fn t31() { ok("fn main() { a := 30 + 1 put(a) }"); }
#[test] fn t32() { ok("fn main() { a := 31 + 1 put(a) }"); }
#[test] fn t33() { ok("fn main() { a := 32 + 1 put(a) }"); }
#[test] fn t34() { ok("fn main() { a := 33 + 1 put(a) }"); }
#[test] fn t35() { ok("fn main() { a := 34 + 1 put(a) }"); }
#[test] fn t36() { ok("fn main() { a := 35 + 1 put(a) }"); }
#[test] fn t37() { ok("fn main() { a := 36 + 1 put(a) }"); }
#[test] fn t38() { ok("fn main() { a := 37 + 1 put(a) }"); }
#[test] fn t39() { ok("fn main() { a := 38 + 1 put(a) }"); }
#[test] fn t40() { ok("fn main() { a := 39 + 1 put(a) }"); }
#[test] fn t41() { ok("fn main() { a := 40 + 1 put(a) }"); }
#[test] fn t42() { ok("fn main() { a := 41 + 1 put(a) }"); }
#[test] fn t43() { ok("fn main() { a := 42 + 1 put(a) }"); }
#[test] fn t44() { ok("fn main() { a := 43 + 1 put(a) }"); }
#[test] fn t45() { ok("fn main() { a := 44 + 1 put(a) }"); }
#[test] fn t46() { ok("fn main() { a := 45 + 1 put(a) }"); }
#[test] fn t47() { ok("fn main() { a := 46 + 1 put(a) }"); }
#[test] fn t48() { ok("fn main() { a := 47 + 1 put(a) }"); }
#[test] fn t49() { ok("fn main() { a := 48 + 1 put(a) }"); }
#[test] fn t50() { ok("fn main() { a := 49 + 1 put(a) }"); }
#[test] fn t51() { ok("fn main() { a := 50 + 1 put(a) }"); }
#[test] fn t52() { ok("fn main() { a := 51 + 1 put(a) }"); }
#[test] fn t53() { ok("fn main() { a := 52 + 1 put(a) }"); }
#[test] fn t54() { ok("fn main() { a := 53 + 1 put(a) }"); }
#[test] fn t55() { ok("fn main() { a := 54 + 1 put(a) }"); }
#[test] fn t56() { ok("fn main() { a := 55 + 1 put(a) }"); }
#[test] fn t57() { ok("fn main() { a := 56 + 1 put(a) }"); }
#[test] fn t58() { ok("fn main() { a := 57 + 1 put(a) }"); }
#[test] fn t59() { ok("fn main() { a := 58 + 1 put(a) }"); }
#[test] fn t60() { ok("fn main() { a := 59 + 1 put(a) }"); }
#[test] fn t61() { ok("fn main() { a := 60 + 1 put(a) }"); }
#[test] fn t62() { ok("fn main() { a := 61 + 1 put(a) }"); }
#[test] fn t63() { ok("fn main() { a := 62 + 1 put(a) }"); }
#[test] fn t64() { ok("fn main() { a := 63 + 1 put(a) }"); }
#[test] fn t65() { ok("fn main() { a := 64 + 1 put(a) }"); }
#[test] fn t66() { ok("fn main() { a := 65 + 1 put(a) }"); }
#[test] fn t67() { ok("fn main() { a := 66 + 1 put(a) }"); }
#[test] fn t68() { ok("fn main() { a := 67 + 1 put(a) }"); }
#[test] fn t69() { ok("fn main() { a := 68 + 1 put(a) }"); }
#[test] fn t70() { ok("fn main() { a := 69 + 1 put(a) }"); }
#[test] fn t71() { ok("fn main() { a := 70 + 1 put(a) }"); }
#[test] fn t72() { ok("fn main() { a := 71 + 1 put(a) }"); }
#[test] fn t73() { ok("fn main() { a := 72 + 1 put(a) }"); }
#[test] fn t74() { ok("fn main() { a := 73 + 1 put(a) }"); }
#[test] fn t75() { ok("fn main() { a := 74 + 1 put(a) }"); }
#[test] fn t76() { ok("fn main() { a := 75 + 1 put(a) }"); }
#[test] fn t77() { ok("fn main() { a := 76 + 1 put(a) }"); }
#[test] fn t78() { ok("fn main() { a := 77 + 1 put(a) }"); }
#[test] fn t79() { ok("fn main() { a := 78 + 1 put(a) }"); }
#[test] fn t80() { ok("fn main() { a := 79 + 1 put(a) }"); }
#[test] fn t81() { ok("fn main() { a := 80 + 1 put(a) }"); }
#[test] fn t82() { ok("fn main() { a := 81 + 1 put(a) }"); }
#[test] fn t83() { ok("fn main() { a := 82 + 1 put(a) }"); }
#[test] fn t84() { ok("fn main() { a := 83 + 1 put(a) }"); }
#[test] fn t85() { ok("fn main() { a := 84 + 1 put(a) }"); }
#[test] fn t86() { ok("fn main() { a := 85 + 1 put(a) }"); }
#[test] fn t87() { ok("fn main() { a := 86 + 1 put(a) }"); }
#[test] fn t88() { ok("fn main() { a := 87 + 1 put(a) }"); }
#[test] fn t89() { ok("fn main() { a := 88 + 1 put(a) }"); }
#[test] fn t90() { ok("fn main() { a := 89 + 1 put(a) }"); }
#[test] fn t91() { ok("fn main() { a := 90 + 1 put(a) }"); }
#[test] fn t92() { ok("fn main() { a := 91 + 1 put(a) }"); }
#[test] fn t93() { ok("fn main() { a := 92 + 1 put(a) }"); }
#[test] fn t94() { ok("fn main() { a := 93 + 1 put(a) }"); }
#[test] fn t95() { ok("fn main() { a := 94 + 1 put(a) }"); }
#[test] fn t96() { ok("fn main() { a := 95 + 1 put(a) }"); }
#[test] fn t97() { ok("fn main() { a := 96 + 1 put(a) }"); }
#[test] fn t98() { ok("fn main() { a := 97 + 1 put(a) }"); }
#[test] fn t99() { ok("fn main() { a := 98 + 1 put(a) }"); }
#[test] fn t100() { ok("fn main() { a := 99 + 1 put(a) }"); }
#[test] fn t101() { ok("fn main() { if 0 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t102() { ok("fn main() { if 1 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t103() { ok("fn main() { if 2 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t104() { ok("fn main() { if 3 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t105() { ok("fn main() { if 4 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t106() { ok("fn main() { if 5 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t107() { ok("fn main() { if 6 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t108() { ok("fn main() { if 7 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t109() { ok("fn main() { if 8 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t110() { ok("fn main() { if 9 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t111() { ok("fn main() { if 10 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t112() { ok("fn main() { if 11 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t113() { ok("fn main() { if 12 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t114() { ok("fn main() { if 13 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t115() { ok("fn main() { if 14 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t116() { ok("fn main() { if 15 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t117() { ok("fn main() { if 16 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t118() { ok("fn main() { if 17 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t119() { ok("fn main() { if 18 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t120() { ok("fn main() { if 19 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t121() { ok("fn main() { if 20 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t122() { ok("fn main() { if 21 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t123() { ok("fn main() { if 22 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t124() { ok("fn main() { if 23 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t125() { ok("fn main() { if 24 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t126() { ok("fn main() { if 25 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t127() { ok("fn main() { if 26 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t128() { ok("fn main() { if 27 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t129() { ok("fn main() { if 28 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t130() { ok("fn main() { if 29 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t131() { ok("fn main() { if 30 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t132() { ok("fn main() { if 31 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t133() { ok("fn main() { if 32 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t134() { ok("fn main() { if 33 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t135() { ok("fn main() { if 34 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t136() { ok("fn main() { if 35 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t137() { ok("fn main() { if 36 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t138() { ok("fn main() { if 37 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t139() { ok("fn main() { if 38 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t140() { ok("fn main() { if 39 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t141() { ok("fn main() { if 40 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t142() { ok("fn main() { if 41 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t143() { ok("fn main() { if 42 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t144() { ok("fn main() { if 43 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t145() { ok("fn main() { if 44 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t146() { ok("fn main() { if 45 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t147() { ok("fn main() { if 46 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t148() { ok("fn main() { if 47 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t149() { ok("fn main() { if 48 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t150() { ok("fn main() { if 49 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t151() { ok("fn main() { if 50 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t152() { ok("fn main() { if 51 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t153() { ok("fn main() { if 52 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t154() { ok("fn main() { if 53 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t155() { ok("fn main() { if 54 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t156() { ok("fn main() { if 55 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t157() { ok("fn main() { if 56 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t158() { ok("fn main() { if 57 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t159() { ok("fn main() { if 58 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t160() { ok("fn main() { if 59 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t161() { ok("fn main() { if 60 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t162() { ok("fn main() { if 61 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t163() { ok("fn main() { if 62 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t164() { ok("fn main() { if 63 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t165() { ok("fn main() { if 64 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t166() { ok("fn main() { if 65 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t167() { ok("fn main() { if 66 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t168() { ok("fn main() { if 67 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t169() { ok("fn main() { if 68 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t170() { ok("fn main() { if 69 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t171() { ok("fn main() { if 70 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t172() { ok("fn main() { if 71 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t173() { ok("fn main() { if 72 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t174() { ok("fn main() { if 73 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t175() { ok("fn main() { if 74 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t176() { ok("fn main() { if 75 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t177() { ok("fn main() { if 76 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t178() { ok("fn main() { if 77 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t179() { ok("fn main() { if 78 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t180() { ok("fn main() { if 79 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t181() { ok("fn main() { if 80 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t182() { ok("fn main() { if 81 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t183() { ok("fn main() { if 82 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t184() { ok("fn main() { if 83 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t185() { ok("fn main() { if 84 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t186() { ok("fn main() { if 85 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t187() { ok("fn main() { if 86 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t188() { ok("fn main() { if 87 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t189() { ok("fn main() { if 88 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t190() { ok("fn main() { if 89 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t191() { ok("fn main() { if 90 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t192() { ok("fn main() { if 91 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t193() { ok("fn main() { if 92 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t194() { ok("fn main() { if 93 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t195() { ok("fn main() { if 94 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t196() { ok("fn main() { if 95 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t197() { ok("fn main() { if 96 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t198() { ok("fn main() { if 97 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t199() { ok("fn main() { if 98 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t200() { ok("fn main() { if 99 > 0 { put(1) } else { put(0) } }"); }
#[test] fn t201() { ok("fn main() { for i in 0..1 { put(i) } }"); }
#[test] fn t202() { ok("fn main() { for i in 0..2 { put(i) } }"); }
#[test] fn t203() { ok("fn main() { for i in 0..3 { put(i) } }"); }
#[test] fn t204() { ok("fn main() { for i in 0..4 { put(i) } }"); }
#[test] fn t205() { ok("fn main() { for i in 0..5 { put(i) } }"); }
#[test] fn t206() { ok("fn main() { for i in 0..6 { put(i) } }"); }
#[test] fn t207() { ok("fn main() { for i in 0..7 { put(i) } }"); }
#[test] fn t208() { ok("fn main() { for i in 0..8 { put(i) } }"); }
#[test] fn t209() { ok("fn main() { for i in 0..9 { put(i) } }"); }
#[test] fn t210() { ok("fn main() { for i in 0..10 { put(i) } }"); }
#[test] fn t211() { ok("fn main() { for i in 0..11 { put(i) } }"); }
#[test] fn t212() { ok("fn main() { for i in 0..12 { put(i) } }"); }
#[test] fn t213() { ok("fn main() { for i in 0..13 { put(i) } }"); }
#[test] fn t214() { ok("fn main() { for i in 0..14 { put(i) } }"); }
#[test] fn t215() { ok("fn main() { for i in 0..15 { put(i) } }"); }
#[test] fn t216() { ok("fn main() { for i in 0..16 { put(i) } }"); }
#[test] fn t217() { ok("fn main() { for i in 0..17 { put(i) } }"); }
#[test] fn t218() { ok("fn main() { for i in 0..18 { put(i) } }"); }
#[test] fn t219() { ok("fn main() { for i in 0..19 { put(i) } }"); }
#[test] fn t220() { ok("fn main() { for i in 0..20 { put(i) } }"); }
#[test] fn t221() { ok("fn main() { for i in 0..21 { put(i) } }"); }
#[test] fn t222() { ok("fn main() { for i in 0..22 { put(i) } }"); }
#[test] fn t223() { ok("fn main() { for i in 0..23 { put(i) } }"); }
#[test] fn t224() { ok("fn main() { for i in 0..24 { put(i) } }"); }
#[test] fn t225() { ok("fn main() { for i in 0..25 { put(i) } }"); }
#[test] fn t226() { ok("fn main() { for i in 0..26 { put(i) } }"); }
#[test] fn t227() { ok("fn main() { for i in 0..27 { put(i) } }"); }
#[test] fn t228() { ok("fn main() { for i in 0..28 { put(i) } }"); }
#[test] fn t229() { ok("fn main() { for i in 0..29 { put(i) } }"); }
#[test] fn t230() { ok("fn main() { for i in 0..30 { put(i) } }"); }
#[test] fn t231() { ok("fn main() { for i in 0..31 { put(i) } }"); }
#[test] fn t232() { ok("fn main() { for i in 0..32 { put(i) } }"); }
#[test] fn t233() { ok("fn main() { for i in 0..33 { put(i) } }"); }
#[test] fn t234() { ok("fn main() { for i in 0..34 { put(i) } }"); }
#[test] fn t235() { ok("fn main() { for i in 0..35 { put(i) } }"); }
#[test] fn t236() { ok("fn main() { for i in 0..36 { put(i) } }"); }
#[test] fn t237() { ok("fn main() { for i in 0..37 { put(i) } }"); }
#[test] fn t238() { ok("fn main() { for i in 0..38 { put(i) } }"); }
#[test] fn t239() { ok("fn main() { for i in 0..39 { put(i) } }"); }
#[test] fn t240() { ok("fn main() { for i in 0..40 { put(i) } }"); }
#[test] fn t241() { ok("fn main() { for i in 0..41 { put(i) } }"); }
#[test] fn t242() { ok("fn main() { for i in 0..42 { put(i) } }"); }
#[test] fn t243() { ok("fn main() { for i in 0..43 { put(i) } }"); }
#[test] fn t244() { ok("fn main() { for i in 0..44 { put(i) } }"); }
#[test] fn t245() { ok("fn main() { for i in 0..45 { put(i) } }"); }
#[test] fn t246() { ok("fn main() { for i in 0..46 { put(i) } }"); }
#[test] fn t247() { ok("fn main() { for i in 0..47 { put(i) } }"); }
#[test] fn t248() { ok("fn main() { for i in 0..48 { put(i) } }"); }
#[test] fn t249() { ok("fn main() { for i in 0..49 { put(i) } }"); }
#[test] fn t250() { ok("fn main() { for i in 0..50 { put(i) } }"); }
#[test] fn t251() { ok("fn main() { for i in 0..51 { put(i) } }"); }
#[test] fn t252() { ok("fn main() { for i in 0..52 { put(i) } }"); }
#[test] fn t253() { ok("fn main() { for i in 0..53 { put(i) } }"); }
#[test] fn t254() { ok("fn main() { for i in 0..54 { put(i) } }"); }
#[test] fn t255() { ok("fn main() { for i in 0..55 { put(i) } }"); }
#[test] fn t256() { ok("fn main() { for i in 0..56 { put(i) } }"); }
#[test] fn t257() { ok("fn main() { for i in 0..57 { put(i) } }"); }
#[test] fn t258() { ok("fn main() { for i in 0..58 { put(i) } }"); }
#[test] fn t259() { ok("fn main() { for i in 0..59 { put(i) } }"); }
#[test] fn t260() { ok("fn main() { for i in 0..60 { put(i) } }"); }
#[test] fn t261() { ok("fn main() { for i in 0..61 { put(i) } }"); }
#[test] fn t262() { ok("fn main() { for i in 0..62 { put(i) } }"); }
#[test] fn t263() { ok("fn main() { for i in 0..63 { put(i) } }"); }
#[test] fn t264() { ok("fn main() { for i in 0..64 { put(i) } }"); }
#[test] fn t265() { ok("fn main() { for i in 0..65 { put(i) } }"); }
#[test] fn t266() { ok("fn main() { for i in 0..66 { put(i) } }"); }
#[test] fn t267() { ok("fn main() { for i in 0..67 { put(i) } }"); }
#[test] fn t268() { ok("fn main() { for i in 0..68 { put(i) } }"); }
#[test] fn t269() { ok("fn main() { for i in 0..69 { put(i) } }"); }
#[test] fn t270() { ok("fn main() { for i in 0..70 { put(i) } }"); }
#[test] fn t271() { ok("fn main() { for i in 0..71 { put(i) } }"); }
#[test] fn t272() { ok("fn main() { for i in 0..72 { put(i) } }"); }
#[test] fn t273() { ok("fn main() { for i in 0..73 { put(i) } }"); }
#[test] fn t274() { ok("fn main() { for i in 0..74 { put(i) } }"); }
#[test] fn t275() { ok("fn main() { for i in 0..75 { put(i) } }"); }
#[test] fn t276() { ok("fn main() { for i in 0..76 { put(i) } }"); }
#[test] fn t277() { ok("fn main() { for i in 0..77 { put(i) } }"); }
#[test] fn t278() { ok("fn main() { for i in 0..78 { put(i) } }"); }
#[test] fn t279() { ok("fn main() { for i in 0..79 { put(i) } }"); }
#[test] fn t280() { ok("fn main() { for i in 0..80 { put(i) } }"); }
#[test] fn t281() { ok("fn main() { for i in 0..81 { put(i) } }"); }
#[test] fn t282() { ok("fn main() { for i in 0..82 { put(i) } }"); }
#[test] fn t283() { ok("fn main() { for i in 0..83 { put(i) } }"); }
#[test] fn t284() { ok("fn main() { for i in 0..84 { put(i) } }"); }
#[test] fn t285() { ok("fn main() { for i in 0..85 { put(i) } }"); }
#[test] fn t286() { ok("fn main() { for i in 0..86 { put(i) } }"); }
#[test] fn t287() { ok("fn main() { for i in 0..87 { put(i) } }"); }
#[test] fn t288() { ok("fn main() { for i in 0..88 { put(i) } }"); }
#[test] fn t289() { ok("fn main() { for i in 0..89 { put(i) } }"); }
#[test] fn t290() { ok("fn main() { for i in 0..90 { put(i) } }"); }
#[test] fn t291() { ok("fn main() { for i in 0..91 { put(i) } }"); }
#[test] fn t292() { ok("fn main() { for i in 0..92 { put(i) } }"); }
#[test] fn t293() { ok("fn main() { for i in 0..93 { put(i) } }"); }
#[test] fn t294() { ok("fn main() { for i in 0..94 { put(i) } }"); }
#[test] fn t295() { ok("fn main() { for i in 0..95 { put(i) } }"); }
#[test] fn t296() { ok("fn main() { for i in 0..96 { put(i) } }"); }
#[test] fn t297() { ok("fn main() { for i in 0..97 { put(i) } }"); }
#[test] fn t298() { ok("fn main() { for i in 0..98 { put(i) } }"); }
#[test] fn t299() { ok("fn main() { for i in 0..99 { put(i) } }"); }
#[test] fn t300() { ok("fn main() { for i in 0..100 { put(i) } }"); }
#[test] fn t301() { ok("fn f(x: int) -> int { x + 1 } fn main() { put(f(1)) }"); }
#[test] fn t302() { ok("fn f(x: int) -> int { x + 2 } fn main() { put(f(1)) }"); }
#[test] fn t303() { ok("fn f(x: int) -> int { x + 3 } fn main() { put(f(1)) }"); }
#[test] fn t304() { ok("fn f(x: int) -> int { x + 4 } fn main() { put(f(1)) }"); }
#[test] fn t305() { ok("fn f(x: int) -> int { x + 5 } fn main() { put(f(1)) }"); }
#[test] fn t306() { ok("fn f(x: int) -> int { x + 6 } fn main() { put(f(1)) }"); }
#[test] fn t307() { ok("fn f(x: int) -> int { x + 7 } fn main() { put(f(1)) }"); }
#[test] fn t308() { ok("fn f(x: int) -> int { x + 8 } fn main() { put(f(1)) }"); }
#[test] fn t309() { ok("fn f(x: int) -> int { x + 9 } fn main() { put(f(1)) }"); }
#[test] fn t310() { ok("fn f(x: int) -> int { x + 10 } fn main() { put(f(1)) }"); }
#[test] fn t311() { ok("fn f(x: int) -> int { x + 11 } fn main() { put(f(1)) }"); }
#[test] fn t312() { ok("fn f(x: int) -> int { x + 12 } fn main() { put(f(1)) }"); }
#[test] fn t313() { ok("fn f(x: int) -> int { x + 13 } fn main() { put(f(1)) }"); }
#[test] fn t314() { ok("fn f(x: int) -> int { x + 14 } fn main() { put(f(1)) }"); }
#[test] fn t315() { ok("fn f(x: int) -> int { x + 15 } fn main() { put(f(1)) }"); }
#[test] fn t316() { ok("fn f(x: int) -> int { x + 16 } fn main() { put(f(1)) }"); }
#[test] fn t317() { ok("fn f(x: int) -> int { x + 17 } fn main() { put(f(1)) }"); }
#[test] fn t318() { ok("fn f(x: int) -> int { x + 18 } fn main() { put(f(1)) }"); }
#[test] fn t319() { ok("fn f(x: int) -> int { x + 19 } fn main() { put(f(1)) }"); }
#[test] fn t320() { ok("fn f(x: int) -> int { x + 20 } fn main() { put(f(1)) }"); }
#[test] fn t321() { ok("fn f(x: int) -> int { x + 21 } fn main() { put(f(1)) }"); }
#[test] fn t322() { ok("fn f(x: int) -> int { x + 22 } fn main() { put(f(1)) }"); }
#[test] fn t323() { ok("fn f(x: int) -> int { x + 23 } fn main() { put(f(1)) }"); }
#[test] fn t324() { ok("fn f(x: int) -> int { x + 24 } fn main() { put(f(1)) }"); }
#[test] fn t325() { ok("fn f(x: int) -> int { x + 25 } fn main() { put(f(1)) }"); }
#[test] fn t326() { ok("fn f(x: int) -> int { x + 26 } fn main() { put(f(1)) }"); }
#[test] fn t327() { ok("fn f(x: int) -> int { x + 27 } fn main() { put(f(1)) }"); }
#[test] fn t328() { ok("fn f(x: int) -> int { x + 28 } fn main() { put(f(1)) }"); }
#[test] fn t329() { ok("fn f(x: int) -> int { x + 29 } fn main() { put(f(1)) }"); }
#[test] fn t330() { ok("fn f(x: int) -> int { x + 30 } fn main() { put(f(1)) }"); }
#[test] fn t331() { ok("fn f(x: int) -> int { x + 31 } fn main() { put(f(1)) }"); }
#[test] fn t332() { ok("fn f(x: int) -> int { x + 32 } fn main() { put(f(1)) }"); }
#[test] fn t333() { ok("fn f(x: int) -> int { x + 33 } fn main() { put(f(1)) }"); }
#[test] fn t334() { ok("fn f(x: int) -> int { x + 34 } fn main() { put(f(1)) }"); }
#[test] fn t335() { ok("fn f(x: int) -> int { x + 35 } fn main() { put(f(1)) }"); }
#[test] fn t336() { ok("fn f(x: int) -> int { x + 36 } fn main() { put(f(1)) }"); }
#[test] fn t337() { ok("fn f(x: int) -> int { x + 37 } fn main() { put(f(1)) }"); }
#[test] fn t338() { ok("fn f(x: int) -> int { x + 38 } fn main() { put(f(1)) }"); }
#[test] fn t339() { ok("fn f(x: int) -> int { x + 39 } fn main() { put(f(1)) }"); }
#[test] fn t340() { ok("fn f(x: int) -> int { x + 40 } fn main() { put(f(1)) }"); }
#[test] fn t341() { ok("fn f(x: int) -> int { x + 41 } fn main() { put(f(1)) }"); }
#[test] fn t342() { ok("fn f(x: int) -> int { x + 42 } fn main() { put(f(1)) }"); }
#[test] fn t343() { ok("fn f(x: int) -> int { x + 43 } fn main() { put(f(1)) }"); }
#[test] fn t344() { ok("fn f(x: int) -> int { x + 44 } fn main() { put(f(1)) }"); }
#[test] fn t345() { ok("fn f(x: int) -> int { x + 45 } fn main() { put(f(1)) }"); }
#[test] fn t346() { ok("fn f(x: int) -> int { x + 46 } fn main() { put(f(1)) }"); }
#[test] fn t347() { ok("fn f(x: int) -> int { x + 47 } fn main() { put(f(1)) }"); }
#[test] fn t348() { ok("fn f(x: int) -> int { x + 48 } fn main() { put(f(1)) }"); }
#[test] fn t349() { ok("fn f(x: int) -> int { x + 49 } fn main() { put(f(1)) }"); }
#[test] fn t350() { ok("fn f(x: int) -> int { x + 50 } fn main() { put(f(1)) }"); }
#[test] fn t351() { ok("fn f(x: int) -> int { x + 51 } fn main() { put(f(1)) }"); }
#[test] fn t352() { ok("fn f(x: int) -> int { x + 52 } fn main() { put(f(1)) }"); }
#[test] fn t353() { ok("fn f(x: int) -> int { x + 53 } fn main() { put(f(1)) }"); }
#[test] fn t354() { ok("fn f(x: int) -> int { x + 54 } fn main() { put(f(1)) }"); }
#[test] fn t355() { ok("fn f(x: int) -> int { x + 55 } fn main() { put(f(1)) }"); }
#[test] fn t356() { ok("fn f(x: int) -> int { x + 56 } fn main() { put(f(1)) }"); }
#[test] fn t357() { ok("fn f(x: int) -> int { x + 57 } fn main() { put(f(1)) }"); }
#[test] fn t358() { ok("fn f(x: int) -> int { x + 58 } fn main() { put(f(1)) }"); }
#[test] fn t359() { ok("fn f(x: int) -> int { x + 59 } fn main() { put(f(1)) }"); }
#[test] fn t360() { ok("fn f(x: int) -> int { x + 60 } fn main() { put(f(1)) }"); }
#[test] fn t361() { ok("fn f(x: int) -> int { x + 61 } fn main() { put(f(1)) }"); }
#[test] fn t362() { ok("fn f(x: int) -> int { x + 62 } fn main() { put(f(1)) }"); }
#[test] fn t363() { ok("fn f(x: int) -> int { x + 63 } fn main() { put(f(1)) }"); }
#[test] fn t364() { ok("fn f(x: int) -> int { x + 64 } fn main() { put(f(1)) }"); }
#[test] fn t365() { ok("fn f(x: int) -> int { x + 65 } fn main() { put(f(1)) }"); }
#[test] fn t366() { ok("fn f(x: int) -> int { x + 66 } fn main() { put(f(1)) }"); }
#[test] fn t367() { ok("fn f(x: int) -> int { x + 67 } fn main() { put(f(1)) }"); }
#[test] fn t368() { ok("fn f(x: int) -> int { x + 68 } fn main() { put(f(1)) }"); }
#[test] fn t369() { ok("fn f(x: int) -> int { x + 69 } fn main() { put(f(1)) }"); }
#[test] fn t370() { ok("fn f(x: int) -> int { x + 70 } fn main() { put(f(1)) }"); }
#[test] fn t371() { ok("fn f(x: int) -> int { x + 71 } fn main() { put(f(1)) }"); }
#[test] fn t372() { ok("fn f(x: int) -> int { x + 72 } fn main() { put(f(1)) }"); }
#[test] fn t373() { ok("fn f(x: int) -> int { x + 73 } fn main() { put(f(1)) }"); }
#[test] fn t374() { ok("fn f(x: int) -> int { x + 74 } fn main() { put(f(1)) }"); }
#[test] fn t375() { ok("fn f(x: int) -> int { x + 75 } fn main() { put(f(1)) }"); }
#[test] fn t376() { ok("fn f(x: int) -> int { x + 76 } fn main() { put(f(1)) }"); }
#[test] fn t377() { ok("fn f(x: int) -> int { x + 77 } fn main() { put(f(1)) }"); }
#[test] fn t378() { ok("fn f(x: int) -> int { x + 78 } fn main() { put(f(1)) }"); }
#[test] fn t379() { ok("fn f(x: int) -> int { x + 79 } fn main() { put(f(1)) }"); }
#[test] fn t380() { ok("fn f(x: int) -> int { x + 80 } fn main() { put(f(1)) }"); }
#[test] fn t381() { ok("fn f(x: int) -> int { x + 81 } fn main() { put(f(1)) }"); }
#[test] fn t382() { ok("fn f(x: int) -> int { x + 82 } fn main() { put(f(1)) }"); }
#[test] fn t383() { ok("fn f(x: int) -> int { x + 83 } fn main() { put(f(1)) }"); }
#[test] fn t384() { ok("fn f(x: int) -> int { x + 84 } fn main() { put(f(1)) }"); }
#[test] fn t385() { ok("fn f(x: int) -> int { x + 85 } fn main() { put(f(1)) }"); }
#[test] fn t386() { ok("fn f(x: int) -> int { x + 86 } fn main() { put(f(1)) }"); }
#[test] fn t387() { ok("fn f(x: int) -> int { x + 87 } fn main() { put(f(1)) }"); }
#[test] fn t388() { ok("fn f(x: int) -> int { x + 88 } fn main() { put(f(1)) }"); }
#[test] fn t389() { ok("fn f(x: int) -> int { x + 89 } fn main() { put(f(1)) }"); }
#[test] fn t390() { ok("fn f(x: int) -> int { x + 90 } fn main() { put(f(1)) }"); }
#[test] fn t391() { ok("fn f(x: int) -> int { x + 91 } fn main() { put(f(1)) }"); }
#[test] fn t392() { ok("fn f(x: int) -> int { x + 92 } fn main() { put(f(1)) }"); }
#[test] fn t393() { ok("fn f(x: int) -> int { x + 93 } fn main() { put(f(1)) }"); }
#[test] fn t394() { ok("fn f(x: int) -> int { x + 94 } fn main() { put(f(1)) }"); }
#[test] fn t395() { ok("fn f(x: int) -> int { x + 95 } fn main() { put(f(1)) }"); }
#[test] fn t396() { ok("fn f(x: int) -> int { x + 96 } fn main() { put(f(1)) }"); }
#[test] fn t397() { ok("fn f(x: int) -> int { x + 97 } fn main() { put(f(1)) }"); }
#[test] fn t398() { ok("fn f(x: int) -> int { x + 98 } fn main() { put(f(1)) }"); }
#[test] fn t399() { ok("fn f(x: int) -> int { x + 99 } fn main() { put(f(1)) }"); }
#[test] fn t400() { ok("fn f(x: int) -> int { x + 100 } fn main() { put(f(1)) }"); }
#[test] fn t401() { ok("fn main() { put(str(1)) }"); }
#[test] fn t402() { ok("fn main() { put(str(2)) }"); }
#[test] fn t403() { ok("fn main() { put(str(3)) }"); }
#[test] fn t404() { ok("fn main() { put(str(4)) }"); }
#[test] fn t405() { ok("fn main() { put(str(5)) }"); }
#[test] fn t406() { ok("fn main() { put(str(6)) }"); }
#[test] fn t407() { ok("fn main() { put(str(7)) }"); }
#[test] fn t408() { ok("fn main() { put(str(8)) }"); }
#[test] fn t409() { ok("fn main() { put(str(9)) }"); }
#[test] fn t410() { ok("fn main() { put(str(10)) }"); }
#[test] fn t411() { ok("fn main() { put(str(11)) }"); }
#[test] fn t412() { ok("fn main() { put(str(12)) }"); }
#[test] fn t413() { ok("fn main() { put(str(13)) }"); }
#[test] fn t414() { ok("fn main() { put(str(14)) }"); }
#[test] fn t415() { ok("fn main() { put(str(15)) }"); }
#[test] fn t416() { ok("fn main() { put(str(16)) }"); }
#[test] fn t417() { ok("fn main() { put(str(17)) }"); }
#[test] fn t418() { ok("fn main() { put(str(18)) }"); }
#[test] fn t419() { ok("fn main() { put(str(19)) }"); }
#[test] fn t420() { ok("fn main() { put(str(20)) }"); }
#[test] fn t421() { ok("fn main() { put(str(21)) }"); }
#[test] fn t422() { ok("fn main() { put(str(22)) }"); }
#[test] fn t423() { ok("fn main() { put(str(23)) }"); }
#[test] fn t424() { ok("fn main() { put(str(24)) }"); }
#[test] fn t425() { ok("fn main() { put(str(25)) }"); }
#[test] fn t426() { ok("fn main() { put(str(26)) }"); }
#[test] fn t427() { ok("fn main() { put(str(27)) }"); }
#[test] fn t428() { ok("fn main() { put(str(28)) }"); }
#[test] fn t429() { ok("fn main() { put(str(29)) }"); }
#[test] fn t430() { ok("fn main() { put(str(30)) }"); }
#[test] fn t431() { ok("fn main() { put(str(31)) }"); }
#[test] fn t432() { ok("fn main() { put(str(32)) }"); }
#[test] fn t433() { ok("fn main() { put(str(33)) }"); }
#[test] fn t434() { ok("fn main() { put(str(34)) }"); }
#[test] fn t435() { ok("fn main() { put(str(35)) }"); }
#[test] fn t436() { ok("fn main() { put(str(36)) }"); }
#[test] fn t437() { ok("fn main() { put(str(37)) }"); }
#[test] fn t438() { ok("fn main() { put(str(38)) }"); }
#[test] fn t439() { ok("fn main() { put(str(39)) }"); }
#[test] fn t440() { ok("fn main() { put(str(40)) }"); }
#[test] fn t441() { ok("fn main() { put(str(41)) }"); }
#[test] fn t442() { ok("fn main() { put(str(42)) }"); }
#[test] fn t443() { ok("fn main() { put(str(43)) }"); }
#[test] fn t444() { ok("fn main() { put(str(44)) }"); }
#[test] fn t445() { ok("fn main() { put(str(45)) }"); }
#[test] fn t446() { ok("fn main() { put(str(46)) }"); }
#[test] fn t447() { ok("fn main() { put(str(47)) }"); }
#[test] fn t448() { ok("fn main() { put(str(48)) }"); }
#[test] fn t449() { ok("fn main() { put(str(49)) }"); }
#[test] fn t450() { ok("fn main() { put(str(50)) }"); }
#[test] fn t451() { ok("fn main() { put(str(51)) }"); }
#[test] fn t452() { ok("fn main() { put(str(52)) }"); }
#[test] fn t453() { ok("fn main() { put(str(53)) }"); }
#[test] fn t454() { ok("fn main() { put(str(54)) }"); }
#[test] fn t455() { ok("fn main() { put(str(55)) }"); }
#[test] fn t456() { ok("fn main() { put(str(56)) }"); }
#[test] fn t457() { ok("fn main() { put(str(57)) }"); }
#[test] fn t458() { ok("fn main() { put(str(58)) }"); }
#[test] fn t459() { ok("fn main() { put(str(59)) }"); }
#[test] fn t460() { ok("fn main() { put(str(60)) }"); }
#[test] fn t461() { ok("fn main() { put(str(61)) }"); }
#[test] fn t462() { ok("fn main() { put(str(62)) }"); }
#[test] fn t463() { ok("fn main() { put(str(63)) }"); }
#[test] fn t464() { ok("fn main() { put(str(64)) }"); }
#[test] fn t465() { ok("fn main() { put(str(65)) }"); }
#[test] fn t466() { ok("fn main() { put(str(66)) }"); }
#[test] fn t467() { ok("fn main() { put(str(67)) }"); }
#[test] fn t468() { ok("fn main() { put(str(68)) }"); }
#[test] fn t469() { ok("fn main() { put(str(69)) }"); }
#[test] fn t470() { ok("fn main() { put(str(70)) }"); }
#[test] fn t471() { ok("fn main() { put(str(71)) }"); }
#[test] fn t472() { ok("fn main() { put(str(72)) }"); }
#[test] fn t473() { ok("fn main() { put(str(73)) }"); }
#[test] fn t474() { ok("fn main() { put(str(74)) }"); }
#[test] fn t475() { ok("fn main() { put(str(75)) }"); }
#[test] fn t476() { ok("fn main() { put(str(76)) }"); }
#[test] fn t477() { ok("fn main() { put(str(77)) }"); }
#[test] fn t478() { ok("fn main() { put(str(78)) }"); }
#[test] fn t479() { ok("fn main() { put(str(79)) }"); }
#[test] fn t480() { ok("fn main() { put(str(80)) }"); }
#[test] fn t481() { ok("fn main() { put(str(81)) }"); }
#[test] fn t482() { ok("fn main() { put(str(82)) }"); }
#[test] fn t483() { ok("fn main() { put(str(83)) }"); }
#[test] fn t484() { ok("fn main() { put(str(84)) }"); }
#[test] fn t485() { ok("fn main() { put(str(85)) }"); }
#[test] fn t486() { ok("fn main() { put(str(86)) }"); }
#[test] fn t487() { ok("fn main() { put(str(87)) }"); }
#[test] fn t488() { ok("fn main() { put(str(88)) }"); }
#[test] fn t489() { ok("fn main() { put(str(89)) }"); }
#[test] fn t490() { ok("fn main() { put(str(90)) }"); }
#[test] fn t491() { ok("fn main() { put(str(91)) }"); }
#[test] fn t492() { ok("fn main() { put(str(92)) }"); }
#[test] fn t493() { ok("fn main() { put(str(93)) }"); }
#[test] fn t494() { ok("fn main() { put(str(94)) }"); }
#[test] fn t495() { ok("fn main() { put(str(95)) }"); }
#[test] fn t496() { ok("fn main() { put(str(96)) }"); }
#[test] fn t497() { ok("fn main() { put(str(97)) }"); }
#[test] fn t498() { ok("fn main() { put(str(98)) }"); }
#[test] fn t499() { ok("fn main() { put(str(99)) }"); }
#[test] fn t500() { ok("fn main() { put(str(100)) }"); }

// ===== v0.0.1d: 前端覆盖新增特性 =====

fn err(src: &str, needle: &str) {
    match build("t.gt", src) {
        Ok(_) => panic!("should fail: {}", src),
        Err(diags) => {
            let msg: Vec<String> = diags.iter().map(|d| d.message.clone()).collect();
            let joined = msg.join("|");
            assert!(joined.contains(needle), "expected {:?} in {:?}", needle, joined);
        }
    }
}

#[test] fn t501() { ok("fn 加一(n: int) -> int { return n + 1 }  fn main() { g := 加一  put(g(1)) }"); }
#[test] fn t502() { ok("fn 加一(n: int) -> int { return n + 1 }  fn 应(f, x: int) -> int { return f(x) }  fn main() { put(应(加一, 1)) }"); }
#[test] fn t503() { ok("fn 加一(n: int) -> int { return n + 1 }  fn 乘二(n: int) -> int { return n * 2 }  fn main() { h := if true { 加一 } else { 乘二 }  put(h(1)) }"); }
#[test] fn t504() { ok("fn 加一(n: int) -> int { return n + 1 }  fn main() { fs := list()  push(fs, 加一)  put(fs[0](1)) }"); }
#[test] fn t505() { ok("fn main() { f := |x: int| x + 1  put(f(1)) }"); }
#[test] fn t506() { ok("fn main() { f := |x: int| |y: int| x + y  g := f(1)  put(g(2)) }"); }
#[test] fn t507() { ok("fn main() { xs := list()  push(xs, 1)  g := |x: int| len(xs) + x  put(g(1)) }"); }
#[test] fn t508() { ok("fn 复合(f, g) { return |x: int| g(f(x)) }  fn main() { inc := |x: int| x + 1  dbl := |x: int| x * 2  h := 复合(inc, dbl)  put(h(1)) }"); }
#[test] fn t509() { ok("fn 恒等[T](x: T) -> T { return x }  fn main() { put(恒等(1))  put(恒等(\"s\")) }"); }
#[test] fn t510() { ok("fn 首[T](xs: list[T]) -> T { return xs[0] }  fn main() { a := list()  push(a, 1)  put(首(a)) }"); }
#[test] fn t511() { ok("fn f(b: int) { if b == 0 { return Err(\"x\") }  return Ok(1) }  fn main() { r := f(0)  match r { Ok(v) => { put(v) }  Err(e) => { put(0) } } }"); }
#[test] fn t512() { ok("fn main() { put(-9223372036854775808) }"); }
#[test] fn t513() { ok("fn main() { i := 0  loop 3 { i = i + 1 }  put(i) }"); }
#[test] fn t514() { ok("fn main() { c := chan()  go 生产者(c)  sleep(1) }  fn 生产者(c) { chan_send(c, 1) }"); }
#[test] fn t515() { ok("fn main() { f := |a: int, b: int| a + b  put(f(1, 2)) }"); }

#[test] fn t516() { err("fn main() { loop { put(1) } }", "loop requires a count"); }
#[test] fn t517() { err("fn main() { x := 1 + ", "expected"); }
#[test] fn t518() { err("fn main() { put(未定义函数(1)) }", "undefined"); }

// `x or y`：Option 默认值（Some 取 v / None 取 y）
#[test] fn t519() { ok("fn f() { return Some(1) }  fn main() { put(f() or 0) }"); }
#[test] fn t520() { ok("fn main() { o := Some(3)  put(o or 9) }"); }
#[test] fn t521() { ok("fn main() { n := None  put(n or 42) }"); }
#[test] fn t522() { ok("fn f(n: int) { if n < 0 { return None }  return Some(n) }  fn main() { put(f(-1) or 7) }"); }
