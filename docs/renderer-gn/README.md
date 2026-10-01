# GN SDK（闭源游戏引擎 SDK）逆向文档

> 对象：`GN_SDK1e`（用户提供的 SDK 分发包，本地 `D:/KF/GN_SDK1e/GN_SDK1e/`）。**闭源**：只给头文件 + 导入库 + DLL + 样例；
> 全部"实现级"知识来自头文件注释、样例代码、DLL 导出表与运行日志——本文档集只做**接口与工程事实**的整理，不做反编译。

## 版本锚与复算

| 项 | 值 | 锚 |
| --- | --- | --- |
| 接口版本 | `2.2.0` | `Include/GN.h:5` |
| 引擎主版本 | `2025.12.7` | `Include/GN.h:7` |
| G2D 模块版本 | `3` | `Include/_Graphics/GN_G2D.h:5` |
| 依赖 | OpenGL / GLEW / OpenAL / Fbx SDK /（可选）FFmpeg | `Include/GN.h:15`–`:16` |
| 许可 | **未随包给出可引用的开源许可**；按"闭源只读参考"处理，**不得移植进 GPL 工程** | 包内无 LICENSE 文件 |

**复算导出计数**（本机 MSYS objdump 实测，2026-10-01）：

```bash
for d in D:/KF/GN_SDK1e/GN_SDK1e/GN*.dll; do
  echo "$d: $(objdump -p "$d" | grep -cE '\[\s*[0-9]+\] \+base\[\s*[0-9]+\]  [0-9a-f]{4} \?')"
done
# 实测：GNwin32.dll 4383 · GN_Gwin32.dll 214 · GN_Sys_win32.dll 69 · GN_Edi_win32.dll 40
#      GN_Sound_win32.dll 37 · GN_Media_win32.dll 27 · GN_Main_win32.dll 22 · GN_Net_win32.dll 4
```

> 导出名是 MSVC C++ 修饰名（`?…@@…`）；`GNwin32.dll` 的 4383 条=整套 API 面（C++ 类方法逐条导出），
> 说明 SDK 以"类库"形态发布而非 C ABI。

## 阅读顺序

| # | 文档 | 内容 |
| --- | --- | --- |
| 01 | [`01-overview.md`](01-overview.md) | 定位、模块位/依赖树、命名法、DLL 分工、样例导读 |
| 02 | [`02-render-layout-font-ui.md`](02-render-layout-font-ui.md) | G2D 绘制模型、布局/显示列表、GB2312 字体图、UI 组件树与序列化、输入框 |
| 03 | [`03-3d-and-notes.md`](03-3d-and-notes.md) | 3D 侧现状、数据格式、对 LSSMJ 的取舍清单、未验证清单 |

## 证据边界（必须带读）

- "源码级"结论只到**头文件声明与注释**（`source` 锚 = `.h`/`.cpp` 行号 + 逐字引文，账本 `../analysis/ledger/w4a.jsonl`）。
- **编码事实**：GN SDK 的整套头文件与文本资源为 **GBK/GB2312 编码**（`RunLog.txt` 亦同）。
  本仓校验器对 UTF-8 解码失败的文件自动回退 GBK 读取（`tools/ledger.py` 的 `read_lines`；
  回退不改变引文逐字匹配强度）；引用时保留原字节语义，本文档正文为 UTF-8。
- **实现全部未读**（无源码）；凡推断处均显式标注"推断"。
- 未运行任何 GN 程序；导出数量经 objdump 复核，但**符号语义未逐个验证**。
