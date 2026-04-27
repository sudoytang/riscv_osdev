.section .text.entry
.global _start

_start:
    la sp, boot_stack_top
    call kernel_main

spin:
    j spin