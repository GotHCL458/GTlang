; GTLang -> LLVM IR（gtc_rust）
target datalayout = "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-msvc"

source_filename = "tmp_neg2.gt"

@.str0 = private unnamed_addr constant [6 x i8] c"hello\00"
@.str1 = private unnamed_addr constant [2 x i8] c"\0A\00"

declare i32 @gt_printf(ptr, ...)
declare i64 @gt_i64_to_int(i64)
declare i64 @gt_int_to_i64(i64)
declare i64 @strlen(ptr)
declare void @gt_bounds(i64, i64, i64)
declare void @gt_int_print(i64)
declare void @gt_rt_init()

define i32 @main() {
entry:
  %v0 = alloca ptr
  call void @gt_rt_init()
  store ptr @.str0, ptr %v0
  %t0 = call i64 @gt_int_to_i64(i64 -2)
  %t1 = call i64 @strlen(ptr @.str0)
  %t2 = icmp slt i64 %t0, 0
  %t3 = add i64 %t0, %t1
  %t4 = select i1 %t2, i64 %t3, i64 %t0
  %t5 = icmp ult i64 %t4, %t1
  br i1 %t5, label %L0, label %L1
L1:
  call void @gt_bounds(i64 %t4, i64 %t1, i64 3)
  unreachable
L0:
  %t6 = getelementptr inbounds i8, ptr @.str0, i64 %t4
  %t7 = load i8, ptr %t6
  %t8 = zext i8 %t7 to i64
  %t9 = call i64 @gt_i64_to_int(i64 %t8)
  call void @gt_int_print(i64 %t9)
  call i32 (ptr, ...) @gt_printf(ptr @.str1)
  ret i32 0
}

