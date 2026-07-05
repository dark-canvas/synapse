.global yield_task
.extern CURRENT_TASK
.text

# this function doesn't take any arguments, and must save all the registers to 
# the current task, determine the next task, and restore it's context.
# To simply test calling assembly from rust, this function currently does nothing...
yield_task:
    push rax
    movq CURRENT_TASK, %rax
    pop rax
    ret
