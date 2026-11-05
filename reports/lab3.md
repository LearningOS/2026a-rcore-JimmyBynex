# Lab3（第五章）实验报告

## 一、实现的功能

移植 `sys_get_time`、`mmap`、`munmap`，内部逻辑不变，改为通过 `current_task()` 取当前进程的地址空间。实现 `sys_spawn`：按 ELF 新建子进程，建立父子关系后加入就绪队列并返回 pid。实现 stride 调度：TCB 加 `priority` 和 `stride`，`fetch` 选出 stride 最小者并加上 `BIG_STRIDE / priority`；`sys_set_priority` 拒绝小于 2 的值。

本地按 CI 流程（BASE=2，`ch5_usertest` 作为 initproc）运行，官方 `check/ch5.py` 判定 15/15 通过。`ch5_stride` 中 6 个进程的"工作量 ÷ 优先级"在 3858533 到 3998066 之间，最大值与最小值之比约为 1.04。

## 二、简答作业

### 1. 8 位 stride 溢出：p1.stride = 255，p2.stride = 250，pass 都是 10

**不会轮到 p1。**

p2 执行一个时间片后，stride 变成 250 + 10 = 260。8 位无符号整数存不下 260，结果绕回 260 − 256 = **4**。

下一次调度时比较 p1 = 255 和 p2 = 4。按无符号数比较，4 < 255，于是又选中了 p2。实际上 p2 已经比 p1 多跑了，理应轮到 p1，但溢出让 p2 的 stride 看起来反而最小。只要 p2 的 stride 还没有再涨过 255，它就会一直被选中，p1 则一直轮不到。

### 2. 为什么优先级全部 >= 2 时，STRIDE_MAX – STRIDE_MIN <= BigStride / 2

优先级 >= 2，所以每个进程的 pass = BigStride / priority <= BigStride / 2。

用归纳法说明这个差值不会超过 BigStride / 2：

- **开始时**：所有进程的 stride 都是 0，差值为 0。
- **每次调度**：假设调度前所有 stride 都落在 [MIN, MAX] 之内，并且 MAX − MIN <= BigStride / 2。调度器选中的是 stride 等于 MIN 的那个进程，它的 stride 变成 MIN + pass <= MIN + BigStride / 2。其余进程不变。
  - 新的最小值 >= 原来的 MIN（被选中的进程变大了，其余进程本来就 >= MIN）。
  - 新的最大值 = max(原来的 MAX, MIN + pass)，这两项都 <= 原来的 MIN + BigStride / 2 <= 新的最小值 + BigStride / 2。

所以每次调度之后，差值仍然不超过 BigStride / 2。

直观地说：每次被加步长的总是当前最落后的那个进程，而一次最多只加 BigStride / 2，所以它永远不会"跑"到比最落后的进程领先超过 BigStride / 2 的位置。

### 3. 补全 `partial_cmp`

由第 2 题，任意两个 stride 的真实差值都不超过 BigStride / 2。只要 BigStride / 2 小于 2^63，就可以把两数做环绕减法，再把结果看成有符号数：

- 结果为负：`self` 真正更小（不管中间有没有发生溢出）。
- 结果为正：`self` 真正更大。

`BinaryHeap` 是最大堆，`pop` 返回"最大"的元素。要让它返回真正最小的 stride，需要把比较结果反过来：

```rust
use core::cmp::Ordering;

struct Stride(u64);

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // the real difference is at most BigStride / 2, so the signed
        // wrapping difference tells which one is really smaller
        let difference = self.0.wrapping_sub(other.0) as i64;
        let real_order = if difference < 0 {
            Ordering::Less
        } else {
            Ordering::Greater
        };
        // BinaryHeap is a max-heap: reverse it so pop() returns the real smallest
        Some(real_order.reverse())
    }
}

impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        false
    }
}
```

用 TIPS 里的例子检验（8 位，BigStride = 255，按真实顺序）：

- 125 与 255：125 − 255 = −130，在 8 位里绕回成 126，是正数，所以 125 "更大"，`125 < 255` 为 false。这是因为两者相差 130，已经超过 BigStride / 2 = 127，说明 125 是 255 溢出之后得到的值。
- 129 与 255：129 − 255 = −126，是负数，所以 `129 < 255` 为 true。

本实验的 `TaskManager::fetch` 用的也是同样的比较方式：`(stride.wrapping_sub(min_stride) as isize) < 0`，并用 `wrapping_add` 累加步长，所以 stride 溢出后调度仍然正确。

## 三、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

   > 使用 AI 助手 Claude（Claude Code）讲解第五章原理（进程与程序的区别、TaskControlBlock 的结构、TaskManager 与 Processor 的分工、idle 控制流与两次 `__switch`、initproc 的创建与伪造的 task_cx/TrapContext、fork/exec/exit/waitpid 的资源分配与回收），协助把 `sys_get_time`、`mmap`、`munmap` 从第四章移植到第五章，在我编写 `sys_spawn` 时指出并修复了父进程 `children` 未记录子进程和返回值错误这两个问题，并协助实现 stride 调度与 `sys_set_priority`。

2. 此外，我也参考了 **以下资料**，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

   > rCore-Tutorial-Guide 第五章：https://learningos.cn/rCore-Tutorial-Guide/chapter5/index.html

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。我未曾也不会向他人（含此后各届同学）复制或公开我的本次实验代码，我有义务妥善保管好它们。我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按"0"分计。

## 四、对本次实验的看法（可选）

这一章的教程从"一个新进程如何运行起来"讲起，而前几章是从用户态与内核态的切换讲起的，衔接上有些跳跃。建议先说明 initproc 是什么、内核如何启动它，再讲 fork 和 exec。

代码与教程中有几处容易让人误解的地方：

- `IDLE_PID`（值为 0）以及退出时打印的 "Idle process" 指的其实是 initproc，与 `Processor` 中的 idle 控制流无关。
- `task.rs` 中 `kernel_stack` 字段的注释写着 "Kernel stack corresponding to PID"，`id.rs` 开头也说内核栈位置"由 PID 决定"，但实际上内核栈有自己独立的分配器 `KSTACK_ALLOCATOR`，与 pid 无关。
- 教程中用户库的 `exec` 只有一个参数，而仓库中的 `user_lib::exec` 有 `path` 和 `args` 两个参数。
- `TaskControlBlock::exec` 没有重置 `heap_bottom` 和 `program_brk`，exec 之后它们仍然是旧程序的值。当前测例不会触发这个问题，但 exec 之后再调用 `sbrk` 时，堆的起点可能是错的。
