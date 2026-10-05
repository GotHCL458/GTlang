; GTLang Boot - stage2_32：16 位 -> 32 位保护模式 -> 跳内核 0x10000
BITS 16
ORG 0x7E00

KERNEL_SECS equ 512   ; 256KB

stage2_start:
    mov si, msg2
    call puts
    mov dl, [0x7DF0]
    ; 分块读内核（每次 64 扇区）
    mov word [dap_k + 2], 64
    mov di, KERNEL_SECS / 64
.krd:
    mov si, dap_k
    mov ah, 0x42
    int 0x13
    jc err2
    ; 下 64 扇区
    add word [dap_k + 4], 32768
    adc word [dap_k + 6], 0
    add word [dap_k + 6], 0x800   ; 每次 32KB = 0x800 段
    add word [dap_k + 8], 64
    adc word [dap_k + 10], 0
    dec di
    jnz .krd
    mov si, msg_load2
    call puts

    cli
    o32 lgdt [gdt_desc]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    db 0x66, 0xEA
    dd pm32
    dw 0x08

BITS 32
pm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x90000
    mov dx, 0x3F8
    mov al, 80          ; 'P'
    out dx, al
    mov eax, 0x10000
    call eax
    cli
    hlt
    jmp $

BITS 16
err2:
    mov si, msg_err2
    call puts
    cli
    hlt
    jmp $

puts:
    push ax
    push dx
.n:
    lodsb
    test al, al
    jz .d
    mov dx, 0x3F8
    out dx, al
    jmp .n
.d:
    pop dx
    pop ax
    ret

align 8
gdt:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF
gdt_desc:
    dw gdt_desc - gdt - 1
    dd gdt
    dd 0

align 8
dap_k:
    db 0x10, 0
    dw KERNEL_SECS
    dw 0x0000
    dw 0x1000
    dq 513

msg2 db '[stage2_32] setup', 13, 10, 0
msg_load2 db '[stage2_32] kernel loaded', 13, 10, 0
msg_err2 db '[stage2_32] disk error', 13, 10, 0
