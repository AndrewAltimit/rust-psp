# me.S -- Media Engine boot stub for PSP kernel mode
#
# The PSP's Media Engine (ME) is a second MIPS R4000 core that runs
# independently of the main CPU. This assembly implements the boot
# sequence required to start the ME and execute a task on it.
#
# Linked via global_asm! in psp/src/me.rs
#
# References:
# - PSPSDK libme (pspdev/pspsdk/src/me/)
# - PSP kernel module reverse engineering (uofw)
# - MIPS R4000 CP0 register definitions

.set noreorder
.set noat

# CP0 register numbers
.equ CP0_STATUS,    12
.equ CP0_CAUSE,     13
.equ CP0_EPC,       14
.equ CP0_CONFIG,    16
.equ CP0_ERROREPC,  30

# Cache operation constants
.equ ICACHE_INDEX_INVALIDATE, 0x00
.equ DCACHE_INDEX_WRITEBACK_INVALIDATE, 0x01

# ME cache parameters (16KB I-cache, 16KB D-cache, 64-byte lines)
.equ ICACHE_SIZE,   16384
.equ DCACHE_SIZE,   16384
.equ CACHE_LINE,    64

# ──────────────────────────────────────────────────────────────────
# _me_boot_entry -- ME coprocessor boot entry point
#
# Called by the main CPU writing to the ME reset vector. When the ME
# starts, it jumps here. This routine:
#   1. Disables interrupts
#   2. Clears the caches
#   3. Sets up the stack pointer
#   4. Calls the task function
#
# Register conventions at entry (set by Rust me_boot wrapper):
#   $a0 = pointer to MeBootParams struct:
#         offset 0: task function pointer
#         offset 4: task argument (i32)
#         offset 8: stack top pointer
# ──────────────────────────────────────────────────────────────────

.section .text.me_boot, "ax", @progbits
.global _me_boot_entry
.type _me_boot_entry, @function
_me_boot_entry:
    # 1. Disable interrupts and set kernel mode
    mtc0    $zero, $CP0_STATUS
    nop
    nop

    # 2. Clear the Cause register
    mtc0    $zero, $CP0_CAUSE
    nop

    # 3. Invalidate entire instruction cache
    li      $t0, 0              # cache index
    li      $t1, ICACHE_SIZE    # total size
_icache_invalidate:
    cache   ICACHE_INDEX_INVALIDATE, 0($t0)
    addiu   $t0, $t0, CACHE_LINE
    bne     $t0, $t1, _icache_invalidate
    nop

    # 4. Writeback and invalidate entire data cache
    li      $t0, 0
    li      $t1, DCACHE_SIZE
_dcache_invalidate:
    cache   DCACHE_INDEX_WRITEBACK_INVALIDATE, 0($t0)
    addiu   $t0, $t0, CACHE_LINE
    bne     $t0, $t1, _dcache_invalidate
    nop

    # 5. Load parameters from the MeBootParams struct
    # $a0 already points to MeBootParams (set by caller)
    lw      $t9, 0($a0)         # t9 = task function pointer
    lw      $a1, 4($a0)         # a1 = task argument
    lw      $sp, 8($a0)         # sp = stack top

    # 6. Set up the task argument in $a0
    move    $a0, $a1

    # 7. Jump to the task function
    jr      $t9
    nop

.size _me_boot_entry, . - _me_boot_entry
