; GTLang boot 库 —— 16 位实现（x86 real mode）
; 用 `gtc --asm16` 自组装（不依赖 nasm）。
; 提供：串口、屏幕、键盘、端口、时间、停机。
BITS 16
ORG 0x0000        ; 由调用方决定加载地址（相对寻址）

; ---- 串口 COM1 ----
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

; ---- 端口 IO ----
boot_outb:
    out dx, al
    ret
boot_inb:
    in al, dx
    ret

; ---- 系统 ----
boot_hlt:
    cli
    hlt
    jmp boot_hlt

boot_reboot:
    mov al, 0xFE
    mov dx, 0x64
    out dx, al
    jmp boot_hlt

boot_shutdown:
    mov dx, 0x604
    mov ax, 0x2000
    out dx, ax
    jmp boot_hlt

; ---- 屏幕（BIOS int 10h）----
boot_clear:
    mov ax, 0x0003
    int 0x10
    ret

boot_putc_at:
    ; al=字符, dh=行, dl=列
    mov ah, 0x02
    int 0x10
    mov ah, 0x0A
    int 0x10
    ret

; ---- 键盘（BIOS int 16h）----
boot_getkey:
    mov ah, 0x00
    int 0x16
    ret

; ---- 时间（BIOS tick，18.2Hz）----
boot_time_ms:
    mov ah, 0x00
    int 0x1A
    mov ax, dx
    ret

boot_sleep_ms:
    ret

; ---- 信息 ----
boot_arch:
    mov ax, 16
    ret

