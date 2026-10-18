; bench_nasm_loop.asm — win64
extern printf
default rel

section .data
fmt:    db "%lld", 10, 0
n_loop: dq 200000000

section .text
global main

main:
    push rbp
    mov rbp, rsp
    sub rsp, 32
    xor rax, rax          ; s = 0
    xor rcx, rcx          ; i = 0
    mov r8, [n_loop]
.loop:
    add rax, rcx
    inc rcx
    cmp rcx, r8
    jl .loop
    mov rdx, rax
    lea rcx, [fmt]
    xor eax, eax
    call printf
    xor eax, eax
    leave
    ret
