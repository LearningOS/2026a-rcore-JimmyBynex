# Lab1（第三章）实验报告

## 一、实现的功能

实现了系统调用 `sys_trace`（编号 410）。在每个任务的任务控制块中增加了一张按 syscall 编号索引的计数表；内核在 `syscall()` 入口、分发之前先给当前任务对应编号的计数加一，因此查询时会把本次 trace 调用也计入，`sys_exit` 也能被统计到。`sys_trace` 根据第一个参数选择功能：0 读取指定地址的一个字节，1 向指定地址写入一个字节，2 返回当前任务调用指定 syscall 的次数，其余情况返回 -1。由于本章还没有地址空间，用户传入的地址可以直接当作指针访问。本地 `make run BASE=2` 下全部测例通过。

## 二、简答作业

### 1. 三个 bad 测例的出错行为

使用的 SBI：**RustSBI 0.4.1**（仓库 `bootloader/rustsbi-qemu.bin`）。运行输出如下：

```
[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
[kernel] IllegalInstruction in application, kernel killed it.
[kernel] IllegalInstruction in application, kernel killed it.
```

- `ch2b_bad_address`：向地址 `0x0` 写入一个字节。该地址不可写，触发 Store/AMO 访问异常（`scause` 为 StoreFault）。内核打印出错地址 `0x0` 和出错指令地址 `0x804003a4`，然后杀掉该应用，切换到下一个任务。
- `ch2b_bad_instructions`：在 U 态执行特权指令 `sret`，触发非法指令异常（IllegalInstruction），被内核杀掉。
- `ch2b_bad_register`：在 U 态用 `csrr` 读取 S 态寄存器 `sstatus`，同样触发非法指令异常，被内核杀掉。

三个应用都没有执行到各自的 `panic!("FAIL...")`，说明 U 态确实无法越权执行指令、访问 S 态寄存器，或写入非法地址。

### 2. `trap.S` 中的 `__alltraps` 与 `__restore`

**(1) L40：刚进入 `__restore` 时，sp 代表什么？`__restore` 的两种使用情景是什么？**

sp 指向**内核栈上的 TrapContext**，也就是即将返回用户态的那个任务的 Trap 上下文的起始地址。

两种使用情景：

- **任务第一次运行**：任务初始化时，TaskContext 被设成 `ra = __restore`、`sp = 内核栈上预先构造好的 TrapContext`。`__switch` 恢复这两个寄存器后执行 `ret`，就跳进了 `__restore`。之后经 `sret` 进入应用入口，sp 设为用户栈顶。
- **Trap 处理完后返回用户态**：`__alltraps` 调用 `trap_handler`，`trap_handler` 返回后顺序执行到 `__restore`。此时内核栈上的栈帧已经弹出，sp 回到 TrapContext 处。如果中途发生了任务切换，被切回的任务也是沿着这条路径返回的。

本章的 `__restore` 开头不再有 `mv sp, a0`，因为上面两种情景进入时，sp 都已经指向正确的 TrapContext 了。

**(2) L43-L48：特殊处理了哪些寄存器？它们对进入用户态有何意义？**

```
ld t0, 32*8(sp)
ld t1, 33*8(sp)
ld t2, 2*8(sp)
csrw sstatus, t0
csrw sepc, t1
csrw sscratch, t2
```

- `sstatus`：其中的 `SPP` 字段决定 `sret` 之后回到哪个特权级（这里是 User）；`SPIE` 决定 `sret` 之后 `SIE` 的值，也就是回到用户态后是否响应 S 态中断。
- `sepc`：`sret` 之后 PC 跳到这里。它可能是应用入口、`ecall` 的下一条指令（syscall 返回时），或者是被中断打断的那条指令。
- `sscratch`：写入用户栈指针（TrapContext 中保存的 x2）。L60 的 `csrrw` 会把它和 sp 交换，使 sp 变成用户栈、sscratch 变成内核栈，为下一次 Trap 做准备。

**(3) L50-L56：为何跳过了 x2 和 x4？**

```
ld x1, 1*8(sp)
ld x3, 3*8(sp)
.set n, 5
.rept 27
   LOAD_GP %n
   .set n, n+1
.endr
```

- `x2` 即 sp。此时 sp 正被用来定位 TrapContext，不能被覆盖。用户 sp 已经在 L45-L48 放进了 sscratch，最后由 L60 的 `csrrw` 换回来。
- `x4` 即 tp（线程指针）。应用程序不使用它，`__alltraps` 也没有保存它，所以不需要恢复。

**(4) L60：`csrrw sp, sscratch, sp` 之后，sp 和 sscratch 分别是什么意义？**

sp 指向**用户栈**（应用的栈指针），sscratch 指向**内核栈**（释放 TrapContext 之后的内核栈顶），供下一次 Trap 时切换回内核栈使用。

**(5) `__restore` 中发生状态切换的是哪一条指令？为何该指令执行之后会进入用户态？**

是 L61 的 `sret`。执行 `sret` 时，CPU 把特权级设为 `sstatus.SPP` 中记录的特权级，这里已设为 User；把 PC 设为 `sepc`；并用 `SPIE` 恢复 `SIE`。因此执行后进入 U 态，从 `sepc` 处继续执行用户程序。

**(6) L13：`csrrw sp, sscratch, sp` 之后，sp 和 sscratch 分别是什么意义？**

sp 指向**内核栈**（当前任务的内核栈顶），sscratch 保存**用户栈指针**。接下来在内核栈上分配 TrapContext，并把 sscratch 中的用户 sp 存进 TrapContext 的 x2 位置。

**(7) 从 U 态进入 S 态是哪一条指令发生的？**

系统调用时是用户程序执行的 **`ecall`** 指令。此外，用户程序执行非法指令、访存出错，或者发生时钟中断时，也会由硬件进入 S 态，并跳转到 `stvec` 指向的 `__alltraps`。

## 三、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

   > 使用 AI 助手 Claude（Claude Code）讲解第三章原理（任务切换、TaskContext、内核栈、时钟中断等），并协助补全 `sys_trace` 及 syscall 计数相关代码。

2. 此外，我也参考了 **以下资料**，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

   > rCore-Tutorial-Guide 第三章：https://learningos.cn/rCore-Tutorial-Guide/chapter3/index.html

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。我未曾也不会向他人（含此后各届同学）复制或公开我的本次实验代码，我有义务妥善保管好它们。我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按"0"分计。

## 四、对本次实验的看法（可选）

教程对代码讲得比较细，但对"系统在运行时是什么样子"讲得偏少。比如任务切换时内核栈上留着哪些内容、TaskContext 和 TrapContext 各自存在哪里、它们之间怎么指向，教程大多只用文字一笔带过，初学时很难在脑中建立完整的图景。

建议增加一张本章的**整体内存布局示意图**：标出内核各段、每个任务的内核栈和用户栈、任务管理器、各应用的加载位置，并画出 TaskContext、TrapContext、栈、应用代码之间的指向关系。再配合一个具体例子（比如一次时钟中断从任务 A 切换到任务 B），逐步展示寄存器和栈的变化。这样"一个任务怎么被暂停、又怎么被恢复"会直观很多。

