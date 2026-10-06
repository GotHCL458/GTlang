	.def	@feat.00;
	.scl	3;
	.type	0;
	.endef
	.globl	@feat.00
@feat.00 = 0
	.att_syntax
	.file	"os_tmp_ast5.gt"
	.def	main;
	.scl	2;
	.type	32;
	.endef
	.text
	.globl	main                            # -- Begin function main
	.p2align	4
main:                                   # @main
.seh_proc main
# %bb.0:                                # %entry
	subq	$88, %rsp
	.seh_stackalloc 88
	.seh_endprologue
	callq	gt_rt_init
	movl	$3, %ecx
	callq	gto_ast_lit_int
	movq	%rax, 40(%rsp)                  # 8-byte Spill
	movq	%rax, 80(%rsp)
	leaq	.L.str1(%rip), %rcx
	leaq	.L.str0(%rip), %rdx
	callq	gt_printf
	leaq	.L.str2(%rip), %rcx
	callq	gt_printf
	movl	$4, %ecx
	callq	gto_ast_lit_int
	movq	%rax, 48(%rsp)                  # 8-byte Spill
	movq	%rax, 72(%rsp)
	leaq	.L.str1(%rip), %rcx
	leaq	.L.str3(%rip), %rdx
	callq	gt_printf
	leaq	.L.str2(%rip), %rcx
	callq	gt_printf
	movq	40(%rsp), %rdx                  # 8-byte Reload
	movq	48(%rsp), %r8                   # 8-byte Reload
	movl	$43, %ecx
	callq	gto_ast_binary
	movq	%rax, 56(%rsp)                  # 8-byte Spill
	movq	%rax, 64(%rsp)
	leaq	.L.str1(%rip), %rcx
	leaq	.L.str4(%rip), %rdx
	callq	gt_printf
	leaq	.L.str2(%rip), %rcx
	callq	gt_printf
	movq	40(%rsp), %rcx                  # 8-byte Reload
	callq	gto_ast_free
	movq	48(%rsp), %rcx                  # 8-byte Reload
	callq	gto_ast_free
	movq	56(%rsp), %rcx                  # 8-byte Reload
	callq	gto_ast_free
	leaq	.L.str1(%rip), %rcx
	leaq	.L.str5(%rip), %rdx
	callq	gt_printf
	leaq	.L.str2(%rip), %rcx
	callq	gt_printf
	xorl	%eax, %eax
	.seh_startepilogue
	addq	$88, %rsp
	.seh_endepilogue
	retq
	.seh_endproc
                                        # -- End function
	.section	.rdata,"dr"
.L.str0:                                # @.str0
	.asciz	"a ok"

.L.str1:                                # @.str1
	.asciz	"%s"

.L.str2:                                # @.str2
	.asciz	"\n"

.L.str3:                                # @.str3
	.asciz	"b ok"

.L.str4:                                # @.str4
	.asciz	"c ok"

.L.str5:                                # @.str5
	.asciz	"done"

	.addrsig
	.addrsig_sym gt_printf
	.addrsig_sym gto_ast_binary
	.addrsig_sym gto_ast_lit_int
	.addrsig_sym gt_rt_init
	.addrsig_sym gto_ast_free
