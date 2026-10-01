	.def	@feat.00;
	.scl	3;
	.type	0;
	.endef
	.globl	@feat.00
@feat.00 = 0
	.att_syntax
	.file	"_p1.gt"
	.def	gt_fib;
	.scl	2;
	.type	32;
	.endef
	.text
	.globl	gt_fib                          # -- Begin function gt_fib
	.p2align	4
gt_fib:                                 # @gt_fib
.seh_proc gt_fib
# %bb.0:                                # %entry
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movq	%rcx, %rax
	cmpq	$1, %rcx
	jle	.LBB0_2
# %bb.1:                                # %L3
	leaq	-1(%rax), %rcx
	movq	%rax, %rsi
	callq	gt_fib
	addq	$-2, %rsi
	movq	%rax, %rdi
	movq	%rsi, %rcx
	callq	gt_fib
	addq	%rdi, %rax
	jo	.LBB0_3
.LBB0_2:                                # %common.ret
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.LBB0_3:                                # %L8
	movl	$1, %ecx
	callq	gt_overflow
	int3
	.seh_endproc
                                        # -- End function
	.def	main;
	.scl	2;
	.type	32;
	.endef
	.globl	main                            # -- Begin function main
	.p2align	4
main:                                   # @main
.seh_proc main
# %bb.0:                                # %entry
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movl	$32, %ecx
	callq	gt_fib
	leaq	.L.str0(%rip), %rcx
	movq	%rax, %rdx
	callq	gt_printf
	leaq	.L.str1(%rip), %rcx
	callq	gt_printf
	xorl	%eax, %eax
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
	.seh_endproc
                                        # -- End function
	.section	.rdata,"dr"
.L.str0:                                # @.str0
	.asciz	"%lld"

.L.str1:                                # @.str1
	.asciz	"\n"

	.addrsig
