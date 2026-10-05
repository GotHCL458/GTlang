; 16 位 boot 库 demo：串口输出 + 打印十进制
BITS 16
ORG 0x7C00

start:
    call boot_serial_init
    mov al, 79
    call boot_serial_putc
    mov al, 75
    call boot_serial_putc
    mov al, 10
    call boot_serial_putc
    ; 打印 16（十进制）
    mov ax, 16
    call print_dec
    mov al, 10
    call boot_serial_putc
    call boot_hlt

; 打印 AX（无符号十进制）
print_dec:
    push ax
    push bx
    push cx
    push dx
    mov cx, 0
    mov bx, 10
.pd_div:
    xor dx, dx
    div bx
    push dx
    inc cx
    test ax, ax
    jnz .pd_div
.pd_out:
    pop dx
    mov al, dl
    add al, 48
    call boot_serial_putc
    loop .pd_out
    pop dx
    pop cx
    pop bx
    pop ax
    ret

boot_serial_init:
    mov dx, 0x3FB
    mov al, 0x80
    out dx, al
    mov dx, 0x3F8
    mov al, 0x03
    out dx, al
    mov dx, 0x3F9
    mov al, 0x00
    out dx, al
    mov dx, 0x3FB
    mov al, 0x03
    out dx, al
    mov dx, 0x3FA
    mov al, 0xC7
    out dx, al
    mov dx, 0x3FC
    mov al, 0x0B
    out dx, al
    ret
boot_serial_putc:
    mov dx, 0x3F8
    out dx, al
    ret
boot_hlt:
    cli
    hlt
    jmp boot_hlt

times 510-($-$$) db 0
dw 0xAA55
