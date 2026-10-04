; GTLang Boot - stage1（MBR，512B）：加载 stage2 -> 跳转
BITS 16
ORG 0x7C00

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti
    mov [0x7DF0], dl     ; 保存 BIOS 启动盘号
    mov si, dap
    mov ah, 0x42
    int 0x13
    jc err
    jmp 0x7E00
err:
    cli
    hlt
    jmp $

align 8
dap:
    db 0x10, 0
    dw 128             ; 64KB
    dw 0x7E00
    dw 0
    dq 1

times 510-($-$$) db 0
dw 0xAA55
