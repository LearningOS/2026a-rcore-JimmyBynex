# Lab4（第六章）实验报告

## 一、实现的功能

移植第五章的 `sys_get_time`、`mmap`、`munmap`、stride 调度和 `sys_set_priority`。`sys_spawn` 改为按名字用 `open_file` 打开 ELF 文件、`read_all` 读入后再创建子进程。

实现 `sys_fstat`、`sys_linkat`、`sys_unlinkat`：

- `File` trait 新增带默认实现（返回 `None`）的 `stat` 方法，只有 `OSInode` 覆盖它，标准输入输出不需要改动。
- easy-fs 的 `Inode` 不存 inode 编号，由 `EasyFileSystem::get_inode_id` 从 DiskInode 的位置反推；类型直接读 DiskInode；nlink 不单独存储，每次扫描根目录，统计指向该编号的目录项数量。
- link 在根目录追加一条名字不同、编号相同的目录项；unlink 删除目录项（用最后一项填补空位），名字全部删除后回收 inode 及其全部数据块和索引块。

本地按 CI 流程（BASE=2，`ch6_usertest` 作为 initproc）运行，官方 `check/ch6.py` 判定全部通过。运行后检查磁盘镜像，两张位图中被占用的 inode 和数据块与根目录可达的文件完全一致，没有泄漏。

## 二、简答作业

### 第六章：root inode 起什么作用？如果它的内容损坏了，会发生什么？

**作用**：root inode（0 号 inode）是查找所有文件的起点。easy-fs 只有一个根目录，根目录的内容就是"名字 → inode 编号"的目录项。按名字打开文件时，先读 root inode 找到目录内容所在的数据块，逐条比较名字得到编号，再按编号算出该文件 DiskInode 的位置。0 号是格式化时约定好的固定编号，所以不需要再去别处记录它的位置。

**损坏的后果**：

- 如果 `size` 或块号（`direct`/`indirect`）损坏，就找不到或者读错目录内容，所有文件都无法按名字打开。`exec`、`spawn` 和启动 initproc 都要按名字打开 ELF，所以系统连第一个用户进程都启动不了。文件数据本身仍在磁盘上，只是没有了入口。
- 如果块号被改成了指向别的文件的数据块，还会把别人的数据当作目录项来解析，得到错误的名字和编号，甚至让 create、link 往错误的块里写目录项，进一步破坏其他文件。
- 如果 `type_` 损坏，不再被识别为目录，`find_inode_id` 中的 `assert!(disk_inode.is_dir())` 会直接让内核 panic。

### 第七章：举出使用 pipe 的一个实际应用的例子

统计文件行数：`cat file.txt | wc -l`。shell 创建一个管道，把 `cat` 的标准输出接到管道的写端，把 `wc` 的标准输入接到读端。`cat` 读出文件内容写入管道，`wc -l` 从管道读入并统计行数。两个程序都不知道对方存在，也不需要临时文件。

日常类似的还有 `ps aux | grep nginx`、`history | tail -20` 等。

### 第七章：多个进程互相通信时，每对进程都建管道太繁琐，请设计一个更易用的机制

设计一个**按 pid 寻址的消息邮箱**：

- 内核为每个进程维护一个邮箱，即一个有长度上限的消息队列，放在 TCB 里。
- `send(pid, buf)`：把 buf 拷贝成一条消息，放进目标进程的邮箱。邮箱满了就返回错误（或阻塞等待）。
- `recv(buf)`：从自己的邮箱取出最早的一条消息，同时得到发送者的 pid。邮箱为空就返回错误（或阻塞等待）。

这样 N 个进程之间不需要 N×(N−1)/2 根管道，只要知道对方的 pid 就能通信，回信时用收到的发送者 pid 即可。进程退出时，内核回收它的邮箱。如果需要一对多，可以再加一个"主题"层：进程订阅主题，发到主题的消息由内核复制到所有订阅者的邮箱。

## 三、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

   > 使用 AI 助手 Claude（Claude Code）讲解第六章原理（virtio 块设备与 MMIO、块缓存与淘汰、easy-fs 的磁盘布局及超级块/位图/inode/目录项的设计动机、DiskInode 的直接与间接索引、Inode 与 OSInode 的分层、open 的各种 flags），协助把第五章的 `sys_get_time`、`mmap`、`munmap`、stride 调度从第五章移植过来，检查我编写的 `sys_spawn`。`sys_fstat`、`sys_linkat`、`sys_unlinkat` 及 easy-fs 中对应的 `get_inode_id`、`inode_id`、`nlink`、`link`、`unlink` 等函数，是在讨论设计方案（`File` trait 加默认方法、ino 由位置反推、nlink 扫描根目录统计）之后由 AI 助手编写并向我讲解的。

2. 此外，我也参考了 **以下资料**，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

   > rCore-Tutorial-Guide 第六章：https://learningos.cn/rCore-Tutorial-Guide/chapter6/index.html
   >
   > rCore-Tutorial-Guide 第七章练习：https://learningos.cn/rCore-Tutorial-Guide/chapter7/3exercise.html

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。我未曾也不会向他人（含此后各届同学）复制或公开我的本次实验代码，我有义务妥善保管好它们。我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按"0"分计。

## 四、对本次实验的看法（可选）

本章内容从最上层的 fd 讲起，但 fd 背后的磁盘、块设备和文件系统都还没有出现，读的时候很难想象数据实际在哪里。按"磁盘 → 块缓存 → 磁盘布局 → inode → 目录 → 内核文件 → fd"从下往上讲，会更容易理解。

代码与教程中有几处不一致或容易误解的地方：

- 教程中打开文件的系统调用是 `sys_openat(dirfd, path, flags, mode)`，内核里实际是 `sys_open(path, flags)`，分发时跳过了 dirfd。
- 练习页写的是实现 `sys_stat`，实际要实现的是按 fd 查询的 `sys_fstat`（调用号 80）。
- 教程说 `loader.rs` 已经移除，但 `os/src/` 下仍留有 `loader.rs`、`link_app.S`、`batch.rs`，只是不再参与编译。
- 教程说 `OSInode` 的锁"保证两个进程无法同时访问同一个文件"。实际上这把锁是每次 open 一把，两次独立的 open 互不影响；真正保护磁盘结构的是 `EasyFileSystem` 的全局锁。
- easy-fs 的 `Inode` 不记录自己的 inode 编号，而 fstat 需要 `ino`，只能从位置反推或者额外存储，教程没有提到这一点。
