BITS 16
ORG 0x7C00
start:
    mov ax, 0x1234
    mov bx, ax
    add bx, 1
    cmp bx, 0x1235
    je ok
    jmp fail
ok:
    mov al, 42
    mov dx, 0x3F8
    out dx, al
    jmp done
fail:
    mov al, 33
    mov dx, 0x3F8
    out dx, al
done:
    hlt
