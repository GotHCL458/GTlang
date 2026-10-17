; GTLang -> LLVM IR（gtc_rust）
target datalayout = "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-msvc"

source_filename = "tmp_dbg2.gt"

@.str0 = private unnamed_addr constant [2 x i8] c"[\00"
@.str1 = private unnamed_addr constant [3 x i8] c", \00"
@.str2 = private unnamed_addr constant [2 x i8] c"]\00"
@.str3 = private unnamed_addr constant [2 x i8] c"\0A\00"

declare i32 @gt_printf(ptr, ...)
declare i64 @gt_int_to_i64(i64)
declare void @gt_int_print(i64)
declare void @gt_rt_init()

define i32 @main() {
entry:
  %v0 = alloca [3 x i64]
  %v1 = alloca ptr
  call void @gt_rt_init()
  %t0 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 0
  store i64 6, ptr %t0
  %t1 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 1
  store i64 2, ptr %t1
  %t2 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 2
  store i64 8, ptr %t2
  store ptr %v0, ptr %v1
  call i32 (ptr, ...) @gt_printf(ptr @.str0)
  %t3 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 0
  %t4 = load i64, ptr %t3
  call void @gt_int_print(i64 %t4)
  call i32 (ptr, ...) @gt_printf(ptr @.str1)
  %t5 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 1
  %t6 = load i64, ptr %t5
  call void @gt_int_print(i64 %t6)
  call i32 (ptr, ...) @gt_printf(ptr @.str1)
  %t7 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 2
  %t8 = load i64, ptr %t7
  call void @gt_int_print(i64 %t8)
  call i32 (ptr, ...) @gt_printf(ptr @.str2)
  call i32 (ptr, ...) @gt_printf(ptr @.str3)
  call void @gt_int_print(i64 3)
  call i32 (ptr, ...) @gt_printf(ptr @.str3)
  %t9 = call i64 @gt_int_to_i64(i64 0)
  %t10 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 %t9
  %t11 = load i64, ptr %t10
  call void @gt_int_print(i64 %t11)
  call i32 (ptr, ...) @gt_printf(ptr @.str3)
  %t12 = call i64 @gt_int_to_i64(i64 2)
  %t13 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 %t12
  %t14 = load i64, ptr %t13
  call void @gt_int_print(i64 %t14)
  call i32 (ptr, ...) @gt_printf(ptr @.str3)
  %t15 = call i64 @gt_int_to_i64(i64 4)
  %t16 = getelementptr inbounds [3 x i64], ptr %v0, i64 0, i64 %t15
  %t17 = load i64, ptr %t16
  call void @gt_int_print(i64 %t17)
  call i32 (ptr, ...) @gt_printf(ptr @.str3)
  ret i32 0
}

