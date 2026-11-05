# Lab5（第八章）实验报告

## 一、实现的功能

实现死锁检测：PCB 增加检测开关，以及锁、信号量各一份 Available / Allocation / Need 表；`sys_enable_deadlock_detect` 负责设置开关。lock / down 先登记 Need，再做安全性检查，不安全就撤回并返回 `-0xDEAD`；安全则调用原语，返回后把 Need 转为 Allocation。unlock / up 归还 Allocation。记账全部在系统调用层完成，原有同步原语不变。另移植了 `sys_get_time`。

完成本次实验所用时间：约 6 小时（分 5 天完成，大部分时间用于学习本章原理）。

本地按 CI 流程（BASE=2，`ch8_usertest` 作为 initproc）运行，官方 `check/ch8.py` 判定全部通过。

## 二、简答作业

### 1. 主线程退出时，需要回收哪些资源？其他线程的 TaskControlBlock 可能在哪些位置被引用，是否需要回收？

**需要回收的资源**（对应 `os/src/task/mod.rs` 中的 `exit_current_and_run_next`）：

- **每个线程的用户态资源**（`TaskUserRes`）：tid、用户栈、TrapContext 页。代码先把所有线程的 `res` 取出放进 `recycle_res`，释放 PCB 的借用后再统一 drop，由 `TaskUserRes` 的 `Drop` 归还 tid 并解除映射。这一步必须在回收地址空间之前完成，否则同一批页面会被释放两次。
- **进程的地址空间**：`recycle_data_pages` 回收程序的代码段、数据段等物理页。页表本身所在的页留到 PCB 被释放时回收。
- **文件描述符表**：`fd_table.clear()`，引用计数归零的文件随之关闭。
- **子进程关系**：把子进程交给 initproc 收养，再清空 `children`。
- **各线程的内核栈和 TCB**：`tasks.clear()` 释放 PCB 持有的引用。当前正在退出的主线程还在使用自己的内核栈，所以先通过 `add_stopping_task` 暂存起来，等下一个主线程退出时再释放。
- PCB 本身（含 pid、锁 / 信号量 / 条件变量列表、页表）由父进程 `waitpid` 回收。

**其他线程的 TCB 可能被引用的位置**：

| 位置 | 是否需要处理 | 原因 |
|---|---|---|
| PCB 的 `tasks` 列表 | 需要，`tasks.clear()` | 这是 TCB 的主要所有者，不清除内核栈就不会释放 |
| 就绪队列 `TASK_MANAGER.ready_queue` | 需要，`remove_task` | 否则调度器还会选中已退出进程的线程，访问已回收的地址空间 |
| 定时器队列 `TIMERS` | 需要，`remove_timer` | 否则 sleep 到期后会被 `wakeup_task` 重新放回就绪队列，问题同上 |
| 锁 / 信号量 / 条件变量的等待队列 | 不需要单独处理 | 这些对象只属于本进程，放在 PCB 的列表里，PCB 释放时会随之释放，队列里的引用也就没了；在此之前这些线程不在就绪队列里，不会被调度 |
| `Processor.current` | 不需要 | 单核下正在运行的只有当前退出的主线程，它已经被 `take_current_task` 取走 |
| `TASK_MANAGER.stop_task` | 不需要 | 只存放主线程，作用就是暂时保住它的内核栈 |

TCB 通过 `Weak` 指向 PCB，所以等待队列里的 `Arc<TaskControlBlock>` 不会和 PCB 形成循环引用，PCB 释放时这些 TCB 能被正常回收。

### 2. 对比 Mutex1 和 Mutex2 的实现，二者有什么区别？可能导致什么问题？

**区别**：

- **Mutex1**：unlock 先把 `locked` 设为 false，再叫醒一个等待者。被叫醒的线程回到 `loop` 开头**重新抢锁**。
- **Mutex2**：unlock 有等待者时**不改 `locked`**，相当于把锁直接交给被叫醒的线程；被叫醒的线程从 `block_current_and_run_next` 返回后直接认为自己已经拿到了锁，不再检查。只有没人等待时才设为 false。

**可能导致的问题**：

- Mutex1 会让**后来的线程插队**：被叫醒的线程只是进了就绪队列，还没开始运行。如果这期间别的线程来 lock，看到 `locked == false` 就直接拿走了锁。被叫醒的线程运行后发现锁又被占了，只好重新排到等待队列**末尾**。这对等待者不公平，可能反复被插队而**饥饿**，同时还多了无用的唤醒和上下文切换。
- Mutex2 是先来先得的 FIFO，比较公平，但也有代价：锁交接以后，在被叫醒的线程真正被调度之前，锁一直处于占用状态，其他线程即使能立刻用上也只能等，吞吐量会下降（lock convoy）。另外，它的正确性依赖于「被叫醒就等于拿到锁」：如果线程因为别的原因被唤醒（虚假唤醒），它也会以为自己拿到了锁，导致两个线程同时进入临界区。Mutex1 因为醒来后会重新检查，不存在这个问题。

## 三、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

   > 使用 AI 助手 Claude（Claude Code）讲解第八章原理（引入线程的原因、PCB 与 TCB 的拆分、线程的地址空间布局与资源回收、waittid、竞态条件、MutexSpin 与 MutexBlocking、信号量、条件变量），讲解死锁检测算法及测试用例。在讨论设计方案（在系统调用层记账、被唤醒线程自己把 Need 转为 Allocation、使用 Vec 并按需扩表、锁和信号量共用一个表结构）之后，由 AI 助手编写了检测开关、`DeadlockTable`（`ensure`、`is_safe`）以及各系统调用中的记账与检测代码，并逐段向我讲解；`sys_get_time` 由 AI 助手从我之前章节的实现移植过来。

2. 此外，我也参考了 **以下资料**，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

   > rCore-Tutorial-Guide 第八章：https://learningos.cn/rCore-Tutorial-Guide/chapter8/index.html
   >
   > rCore-Tutorial-Guide 第八章练习：https://learningos.cn/rCore-Tutorial-Guide/chapter8/5exercise.html

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。我未曾也不会向他人（含此后各届同学）复制或公开我的本次实验代码，我有义务妥善保管好它们。我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按"0"分计。

## 四、对本次实验的看法（可选）

本章的结构变化（TCB 拆成 PCB 和 TCB）是理解后续内容的前提，但教程直接从结构体定义讲起。先讲「多个执行流共用一个地址空间」时用户栈、TrapContext、内核栈分别放在哪里，再看结构体，会更容易理解。

代码与教程中有几处不一致或容易误解的地方：

- 教程中信号量 up 和互斥锁 unlock 唤醒等待者时调用的是 `add_task`，代码里是 `wakeup_task`（先把状态设为 Ready，再加入就绪队列）。
- 练习页中 `enable_deadlock_detect` 的 Rust 接口参数是 `i32`，用户库实际是 `bool`，转成 `usize` 后传给内核。
- 练习页把 Need 定义为「线程还需要的资源数量」，这是银行家算法里「最大需求减去已分配」的含义；测试实际只要求记录线程当前正在请求的资源。
- `ch8_deadlock_sem1` 中，检测到死锁时打印的是 `Deadlock detected. Test 08_sem1 failed!`，但这正是预期行为，测试最终仍会通过，提示语容易让人误会。
- 框架自带的 `ch8b_coroutines` 在未修改的代码上运行也会以 -4 退出，它不在评测列表中。
