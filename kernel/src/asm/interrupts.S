/* fantuan-kernel — interrupts.S (M1)
 * ISR stubs: one per vector. Each stub pushes a dummy error code (or relies
 * on the CPU-pushed one), then its vector number, then enters isr_common.
 * The dummy keeps the frame layout identical for both paths.
 * Intel syntax; compiled by cc via kernel/build.rs. */

    .intel_syntax noprefix

.macro isr_no_err n
    .global isr_\n
isr_\n:
    push    0           /* dummy error code */
    push    \n          /* vector number */
    jmp     isr_common
.endm

.macro isr_err n
    .global isr_\n
isr_\n:
    push    \n          /* vector number (CPU already pushed the error code) */
    jmp     isr_common
.endm

isr_no_err 0
isr_no_err 1
isr_no_err 2
isr_no_err 3
isr_no_err 4
isr_no_err 5
isr_no_err 6
isr_no_err 7
isr_err 8
isr_no_err 9
isr_err 10
isr_err 11
isr_err 12
isr_err 13
isr_err 14
isr_no_err 15
isr_no_err 16
isr_err 17
isr_no_err 18
isr_no_err 19
isr_no_err 20
isr_err 21
isr_no_err 22
isr_no_err 23
isr_no_err 24
isr_no_err 25
isr_no_err 26
isr_no_err 27
isr_no_err 28
isr_err 29
isr_err 30
isr_no_err 31
isr_no_err 32
isr_no_err 33
isr_no_err 34
isr_no_err 35
isr_no_err 36
isr_no_err 37
isr_no_err 38
isr_no_err 39
isr_no_err 40
isr_no_err 41
isr_no_err 42
isr_no_err 43
isr_no_err 44
isr_no_err 45
isr_no_err 46
isr_no_err 47
isr_no_err 48
isr_no_err 49
isr_no_err 50
isr_no_err 51
isr_no_err 52
isr_no_err 53
isr_no_err 54
isr_no_err 55
isr_no_err 56
isr_no_err 57
isr_no_err 58
isr_no_err 59
isr_no_err 60
isr_no_err 61
isr_no_err 62
isr_no_err 63
isr_no_err 64
isr_no_err 65
isr_no_err 66
isr_no_err 67
isr_no_err 68
isr_no_err 69
isr_no_err 70
isr_no_err 71
isr_no_err 72
isr_no_err 73
isr_no_err 74
isr_no_err 75
isr_no_err 76
isr_no_err 77
isr_no_err 78
isr_no_err 79
isr_no_err 80
isr_no_err 81
isr_no_err 82
isr_no_err 83
isr_no_err 84
isr_no_err 85
isr_no_err 86
isr_no_err 87
isr_no_err 88
isr_no_err 89
isr_no_err 90
isr_no_err 91
isr_no_err 92
isr_no_err 93
isr_no_err 94
isr_no_err 95
isr_no_err 96
isr_no_err 97
isr_no_err 98
isr_no_err 99
isr_no_err 100
isr_no_err 101
isr_no_err 102
isr_no_err 103
isr_no_err 104
isr_no_err 105
isr_no_err 106
isr_no_err 107
isr_no_err 108
isr_no_err 109
isr_no_err 110
isr_no_err 111
isr_no_err 112
isr_no_err 113
isr_no_err 114
isr_no_err 115
isr_no_err 116
isr_no_err 117
isr_no_err 118
isr_no_err 119
isr_no_err 120
isr_no_err 121
isr_no_err 122
isr_no_err 123
isr_no_err 124
isr_no_err 125
isr_no_err 126
isr_no_err 127
isr_no_err 128
isr_no_err 129
isr_no_err 130
isr_no_err 131
isr_no_err 132
isr_no_err 133
isr_no_err 134
isr_no_err 135
isr_no_err 136
isr_no_err 137
isr_no_err 138
isr_no_err 139
isr_no_err 140
isr_no_err 141
isr_no_err 142
isr_no_err 143
isr_no_err 144
isr_no_err 145
isr_no_err 146
isr_no_err 147
isr_no_err 148
isr_no_err 149
isr_no_err 150
isr_no_err 151
isr_no_err 152
isr_no_err 153
isr_no_err 154
isr_no_err 155
isr_no_err 156
isr_no_err 157
isr_no_err 158
isr_no_err 159
isr_no_err 160
isr_no_err 161
isr_no_err 162
isr_no_err 163
isr_no_err 164
isr_no_err 165
isr_no_err 166
isr_no_err 167
isr_no_err 168
isr_no_err 169
isr_no_err 170
isr_no_err 171
isr_no_err 172
isr_no_err 173
isr_no_err 174
isr_no_err 175
isr_no_err 176
isr_no_err 177
isr_no_err 178
isr_no_err 179
isr_no_err 180
isr_no_err 181
isr_no_err 182
isr_no_err 183
isr_no_err 184
isr_no_err 185
isr_no_err 186
isr_no_err 187
isr_no_err 188
isr_no_err 189
isr_no_err 190
isr_no_err 191
isr_no_err 192
isr_no_err 193
isr_no_err 194
isr_no_err 195
isr_no_err 196
isr_no_err 197
isr_no_err 198
isr_no_err 199
isr_no_err 200
isr_no_err 201
isr_no_err 202
isr_no_err 203
isr_no_err 204
isr_no_err 205
isr_no_err 206
isr_no_err 207
isr_no_err 208
isr_no_err 209
isr_no_err 210
isr_no_err 211
isr_no_err 212
isr_no_err 213
isr_no_err 214
isr_no_err 215
isr_no_err 216
isr_no_err 217
isr_no_err 218
isr_no_err 219
isr_no_err 220
isr_no_err 221
isr_no_err 222
isr_no_err 223
isr_no_err 224
isr_no_err 225
isr_no_err 226
isr_no_err 227
isr_no_err 228
isr_no_err 229
isr_no_err 230
isr_no_err 231
isr_no_err 232
isr_no_err 233
isr_no_err 234
isr_no_err 235
isr_no_err 236
isr_no_err 237
isr_no_err 238
isr_no_err 239
isr_no_err 240
isr_no_err 241
isr_no_err 242
isr_no_err 243
isr_no_err 244
isr_no_err 245
isr_no_err 246
isr_no_err 247
isr_no_err 248
isr_no_err 249
isr_no_err 250
isr_no_err 251
isr_no_err 252
isr_no_err 253
isr_no_err 254
isr_no_err 255

isr_common:
    /* Save all GP registers (the frame layout must match InterruptFrame). */
    push    rax
    push    rbx
    push    rcx
    push    rdx
    push    rsi
    push    rdi
    push    rbp
    push    r8
    push    r9
    push    r10
    push    r11
    push    r12
    push    r13
    push    r14
    push    r15

    mov     rdi, rsp        /* arg1 = *mut InterruptFrame */
    mov     rbp, rsp        /* remember the exact frame base */
    and     rsp, -16        /* SysV alignment for the call */
    sub     rsp, 128        /* reserve the ABI red zone for the handler */
    cld
    call    isr_dispatch

    mov     rsp, rbp
    pop     r15
    pop     r14
    pop     r13
    pop     r12
    pop     r11
    pop     r10
    pop     r9
    pop     r8
    pop     rbp
    pop     rdi
    pop     rsi
    pop     rdx
    pop     rcx
    pop     rbx
    pop     rax

    add     rsp, 16         /* vector + error code (or dummy) */
    iretq
