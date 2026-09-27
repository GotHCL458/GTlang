; NASM x86-64 (Windows ABI) - fib + loop_sum
default rel
extern printf
extern strcmp
global main
section .text

fib:
    cmp rcx, 2
    jl .base
    push rbx
    mov rbx, rcx
    lea rcx, [rbx-1]
    call fib
    push rax
    lea rcx, [rbx-2]
    call fib
    pop rbx
    add rax, rbx
    pop rbx
    ret
.base:
    mov rax, rcx
    ret

loop_sum:
    xor rax, rax
    xor rcx, rcx
.l:
    add rax, rcx
    inc rcx
    cmp rcx, 200000000
    jl .l
    ret

main:
    push rbx
    sub rsp, 32
    mov rbx, rdx
    cmp ecx, 2
    jl .do_fib
    mov rcx, [rbx+8]
    lea rdx, [rel sloop]
    call strcmp
    test eax, eax
    je .do_loop
.do_fib:
    mov rcx, 35
    call fib
    mov rdx, rax
    lea rcx, [rel fmt]
    call printf
    xor eax, eax
    add rsp, 32
    pop rbx
    ret
.do_loop:
    call loop_sum
    mov rdx, rax
    lea rcx, [rel fmt]
    call printf
    xor eax, eax
    add rsp, 32
    pop rbx
    ret

section .data
fmt: db '%lld', 10, 0
sloop: db 'loop', 0