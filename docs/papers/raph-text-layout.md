# Levien 2020：Skills for text layout implementors（文本层分层）

> 锚：`W1F-051..054`；URL <https://raphlinus.github.io/text/2020/10/26/text-layout.html>
> 定级：`paper`（权威长文）；作者=Raph Levien（xi/ropey/2D 图形长文的同一作者）。

## 1. 分层（本批最可直接抄的结构）

- 从粗到细："paragraph segmentation as the coarsest granularity, followed by rich text style and
  BiDi analysis, then itemization (coverage by font), then Unicode script, and shaping clusters as
  the finest."（`W1F-051`）——**段 → 样式/BiDi → 字体覆盖 itemization → script → 整形簇**。
- 断行是"平行的第二层级"（作者明确分开处理），不是一个黑盒函数。

**对 LSSMJ**：文本层模块划分直接照此分层命名（我们的单行档可裁到"簇"为止），
文档里写清"我们在哪一层做了裁剪"。

## 2. 两个"不自研"的结论

1. 整形（`W1F-052`）："a very high quality open source implementation exists, in the form of
   HarfBuzz."——把整形当黑盒。
2. 断行（`W1F-054`）："Doing this properly is quite a tricky problem."——作者把断行独立成层，
   并与 Knuth–Plass（w3f `W3F-001`）、UAX#14（w3f `W3F-012`）构成"困难三连"。

## 3. 缓存粒度的折中（对候选窗有直接价值）

- 作者给的做法：用启发式把文本再切到"隐含词边界"以作**布局缓存粒度**；若字体跨边界成形
  （如跨越边界的连字），"the shaping context is simply lost"，作者称这是"a reasonable compromise"
  （`W1F-053`）。
- **对本项目**：候选行天然是"整行缓存"（内容变才重排），恰好**绕开**这个折中损失——
  这是我们的形态优势，值得写进设计理由（`W1F-053`）。

## 对 LSSMJ 文本层的取舍

- **吸收**：分层命名（`W1F-051`）、"整形不自研"（`W1F-052`，与 `harfbuzz-shaping.md` 合并证据）、
  断行独立成层（`W1F-054`）。
- **吸收并反用**：缓存粒度问题（`W1F-053`）——我们的"整行缓存 + 内容版本号"策略因此不需要
  中间级词边界缓存，少一层复杂度（写进 ADR 的"为什么不做子行缓存"）。
- **不吸收**：作者面向编辑器/文档的场景设定（选区、光标、多行滚动），与候选窗形态不同；
  引用其结论时明确适用范围（单段落、静态内容）。
