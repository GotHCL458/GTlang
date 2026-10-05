/* IRQ 汇编 stub：push 中断号 + 调 irq_handler + iretq（x86_64 SysV） */
.text

.macro IRQ_STUB name, num
.global \name
\name:
    push $0
    push $\num
    jmp irq_common
.endm

IRQ_STUB irq0_stub, 32
IRQ_STUB irq1_stub, 33

.global irq_common
irq_common:
    push %rax
    push %rbx
    push %rcx
    push %rdx
    push %rsi
    push %rdi
    push %rbp
    mov 64(%rsp), %rdi     /* 中断号（越过 7 个保存寄存器 + 2 个 push） */
    call irq_handler
    pop %rbp
    pop %rdi
    pop %rsi
    pop %rdx
    pop %rcx
    pop %rbx
    pop %rax
    add $16, %rsp
    iretq
