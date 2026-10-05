; GTLang Boot - stage2：16 位 -> 32 位保护模式 -> 64 位长模式 -> 跳内核 0x10000
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
    add word [dap_k + 4], 32768
    adc word [dap_k + 6], 0
    add word [dap_k + 6], 0x800
    add word [dap_k + 8], 64
    adc word [dap_k + 10], 0
    dec di
    jnz .krd
    mov si, msg_load2
    call puts

    ; 页表：PML4(0x1000)->PDPT(0x2000)->PD(0x3000)->PT(0x4000)，映射前 2MB
    mov edi, 0x1000
    xor eax, eax
    mov ecx, (4096 * 4) / 4
    rep stosd
    mov edi, 0x1000
    mov dword [edi], 0x2003
    mov dword [edi + 0x1000], 0x3003
    mov dword [edi + 0x2000], 0x4003
    lea edi, [edi + 0x3000]
    xor eax, eax
.pt:
    mov edx, eax
    or edx, 0x03
    mov [edi], edx
    add eax, 0x1000
    add edi, 8
    cmp eax, 0x200000
    jb .pt

    cli
    o32 lgdt [gdt_desc]
    ; PAE + CR3
    mov eax, cr4
    or eax, 0x20
    mov cr4, eax
    mov eax, 0x1000
    mov cr3, eax
    ; PE（先进 32 位保护模式）
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
    mov al, 80
    out dx, al
    ; LME
    mov ecx, 0xC0000080
    rdmsr
    or eax, 0x100
    wrmsr
    ; PG
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax
    ; 远跳进 64 位（选择子 0x18，GDT 里 L=1 的代码段）
    jmp 0x18:long64

BITS 64
long64:
    mov ax, 0x20
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, 0x90000
    mov dx, 0x3F8
    mov al, 76
    out dx, al
    mov rax, 0x10000
    call rax
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
    dq 0                            ; 0x00 空
    dq 0x00CF9A000000FFFF           ; 0x08 32 位代码
    dq 0x00CF92000000FFFF           ; 0x10 32 位数据
    dq 0x00AF9A000000FFFF           ; 0x18 64 位代码（L=1）
    dq 0x00AF92000000FFFF           ; 0x20 64 位数据
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

msg2 db '[stage2] setup', 13, 10, 0
msg_load2 db '[stage2] kernel loaded', 13, 10, 0
msg_err2 db '[stage2] disk error', 13, 10, 0
