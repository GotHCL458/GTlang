BITS 16
ORG 0x7C00
start:
    call boot_serial_init
    mov al, 79          ; 'O'
    call boot_serial_putc
    mov al, 75          ; 'K'
    call boot_serial_putc
    mov al, 10
    call boot_serial_putc
    call boot_hlt

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
