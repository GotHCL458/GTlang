; bench_nasm.asm — win64, 输出 via printf
extern printf
extern time
default rel

section .data
fmt:    db "%lld", 10, 0
n_fib:  dq 35
n_loop: dq 200000000
which:  db "fib", 0

section .text
global main

; long long fib(long long n)  — 朴素递归
fib:
    push rbp
    mov rbp, rsp
    cmp rcx, 2
    jl .base
    push rbx
    mov rbx, rcx
    lea rcx, [rbx - 1]
    call fib
    push rax
    lea rcx, [rbx - 2]
    call fib
    pop rdx
    add rax, rdx
    pop rbx
    pop rbp
    ret
.base:
    mov rax, rcx
    pop rbp
    ret

main:
    push rbp
    mov rbp, rsp
    sub rsp, 32          ; shadow space + align
    ; argv[1]? 简单：固定跑两个都计时？这里只按参数区分
    ; 读参数太麻烦；用两个独立 exe？——改为：默认跑 fib，环境变量太繁。
    ; 简化：这个文件编成 fib 版；loop 版另写。
    lea rcx, [n_fib]
    mov rcx, [rcx]
    call fib
    ; printf("%lld\n", rax)
    mov rdx, rax
    lea rcx, [fmt]
    xor eax, eax
    call printf
    xor eax, eax
    leave
    ret
