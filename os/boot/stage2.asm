; GTLang Boot - stage2（16 位 -> 32 位保护模式 -> 跳内核 0x10000）
BITS 16
ORG 0x7E00

KERNEL_SECS equ 64

stage2_start:
    mov si, msg2
    call puts
    ; 用 BIOS 扩展读内核（LBA 129）到 0x10000
    mov dl, [0x7DF0]
    mov si, dap_k
    mov ah, 0x42
    int 0x13
    jc err2
    mov si, msg_load2
    call puts

    cli
    o32 lgdt [gdt_desc]
    ; PE
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
    mov al, 80          ; 'P'：已进入 32 位保护模式
    out dx, al
    mov eax, 0x10000
    call eax            ; 跳内核入口
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
    dq 129

msg2 db '[stage2] setup', 13, 10, 0
msg_load2 db '[stage2] kernel loaded', 13, 10, 0
msg_err2 db '[stage2] disk error', 13, 10, 0
