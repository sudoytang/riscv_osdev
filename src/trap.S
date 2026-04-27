.section .text.trap
.balign 8
.global __trap_entry

__trap_entry:
    call trap_handler

1:
    j 1b