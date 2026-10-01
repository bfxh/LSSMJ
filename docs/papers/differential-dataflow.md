# Differential Dataflow（CIDR 2013 全文）与 Naiad 系谱

> 源：`cidrdb.org/cidr2013/Papers/CIDR13_Paper111.pdf`（McSherry/Murray/Isaacs/Isard, CIDR 2013；抓取
> 2026-10-01，PyMuPDF 抽文本）；题录：Naiad（SOSP'13，DOI 10.1145/2517349.2522738，W3B-084）、
> timely dataflow（CACM'16，DOI 10.1145/2983551，W3B-085）、DBToaster（VLDB'12，DOI 10.14778/2336664.2336670，
> W3B-086）。账本：W3B-076..086。

## 1. 模型：两个与"传统增量"的差别（W3B-076/077/078）

- 主张句："This paper introduces diﬀerential computation, a new ap-proach that generalizes traditional
  models of incremental computation and is particularly useful when applied to iter…"（W3B-076 的截取段）。
- 差别一（版本序）："the state of the computation varies according to a partially ordered set of
  versions rather than a totally ordered sequence of versions as is standard for incremental
  computation"（W3B-077）——多维逻辑时间戳（版本）：同一位置 i 轮输入、j 次迭代的版本，其推导可复用两个前驱方向的既有工作。
- 差别二（保留史）："the set of updates required to re-construct the state at any given version is
  retained in an indexed data-structure, whereas incremental systems typically consolidate each update
  in sequence into the “current” version of the state and then discard the update."（W3B-078）。

## 2. 读数与代价（W3B-079/080/081/082）

- **保留史便宜**："the total number retained for the 24-hour window is only 1.5% more than the set of
  labels,"（W3B-079）——保留全部迭代差分只比增量数据流多 1.5% 状态。
- **输入微变的收益**："The work done updating the sliding window is only 0.003% of the work done in a
  full prioritized re-evaluation."（W3B-080）。
- **复用边界**："is able to re-use state corresponding to the parts of the graph that have not
  changed."（W3B-082）——未变部分直接复用（传统/增量/优先法在输入改动时都得从零重跑）。
- **对既有 IVM 的批评**（三点失败模式）："existing IVM algorithms are not ideal for interactive
  large-scale computation, because they either perform too much work, maintain too much state, or limit
  expressiveness."（W3B-081）——可做自查清单。
- 工程载体："We have implemented diﬀerential dataﬂow in a system called Naiad"（W3B-083）。

## 3. 谱系记录（题录层，W3B-084/085/086）

- Naiad（SOSP'13）承载及时数据流：container="Proceedings of the Twenty-Fourth ACM Symposium on
  Operating Systems Principles"（W3B-084）。
- CACM'16 综述版《Incremental, iterative data processing with timely dataflow》（W3B-085）。
- DBToaster（VLDB'12，高阶 delta 处理）为数据库 IVM 的另一支（W3B-086）。
- 三者均**只核 DOI/题录**，系统实现未读。

## 4. 对 LSSMJ 的取舍

- **不引运行时**：数据流调度、可迭代嵌套、多版本时间戳都超出"单线程帧循环"的需求（W3B-083/085）。
- **取三条概念**：
  1. 变化以**差分集合**表达（行增/删/改）——比"重算 + 比较"更贴近我们的 damage 生成；
  2. **保留前一版**（描述符/显示列表双缓冲）——保留史便宜有实证（W3B-079/080）；
  3. **未变即复用**（W3B-082）——判据 C3"空 damage → 零重画"的一般化。
- **若滚动面板引入迭代式布局**：多维版本（内容修订 × 滚动位置）值得重估（W3B-077 的适用性），
  见设计 §10 未决项。

## 5. 未验证

- 论文 §4（差分数据流的算子语义）与 §5（Naiad 实现细节）未精读；只取模型层与实测数字。
- 0.003%/1.5% 均为**该论文 24 小时 Twitter 图滑窗**这一特定工作负载的单点读数，不可外推为普遍结论。
