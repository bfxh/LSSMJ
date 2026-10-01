# GN SDK 01 · 定位、模块地图与 DLL 分工

## 定位（SDK 自述）

"本引擎SDK是个免费,轻量,依赖少,核心技术多,配置要求低,优化到位,画面优秀,跨平台且保持统一渲染效果的实时3D引擎"
（`Include/GN.h:11`）。依赖 OpenGL/GLEW/OpenAL/Fbx SDK，播放视频另需 FFmpeg（`GN.h:15`–`:16`）。
技术代际可从头文件直接读出：渲染后端清单里包含 **"DirectDraw"、"Direct3D 9"、"OpenGL es1"、"OpenGL 2"**
（`Include/_Graphics/GN_G2D.h:178` 的接口信息注释）——面向"2011 年以前的设备"（`GN.h:174`）的兼容面。

## 模块与编译开关（位集）

`GN.h:35`–`:65` 定义内嵌模块位：`GN_USING_2D=2`（"前台应用必需"）、`USING_3D=4`、`USING_AUDIO=8`、
`USING_NET=16`（"后台服务必需"）、`USING_UNZIP=32`、`USING_UPDATE=64`、`UTILITY_LIB=1`、`ADDITIONAL_LIB=256`、
`EXT_3DFILE_FORMAT=512`、`G3D_FORMAT_SAVE=2048`；运行期用 `GN_GetModuleConstitute()` 读回位集（`GN.h:215`）。
附带两个工程向常量：文件缓冲 2MB（`GN.h:71`）、**最大线程并行数 32**（`GN.h:74`）。

**私有接口纪律**（值得抄的工程实践）：`GN_USING_PRIVATE_INF` 包裹内部接口；SDK 模式下
`GN_RUNTSTART/GN_RunTextOut/...` 一整套调试宏被编译成 `0` 空操作（`GN.h:90`–`:102`），
并明确"`GT_` 开头的功能不建议使用…引擎以后的新版本有可能会移除或更变"（`代码功能简述与规范.txt` 总纲）。

## 模块地图（`GN.h` 的依赖树就是官方"目录索引"）

`GN.h:128`–`:205` 以缩进表达"依赖靠左"的包含关系，每行都带一句职责注释。摘录（左侧为层级）：

```text
GN_Base(随机数/插值) → GN_Array(容器/内存池) → GN_String → GN_Sys(IO/剪贴板/转码/线程/时间)
  → GN_Parallel(并行任务) → GN_File(文件/词条/日志/插件) → GN_Format(格式/颜色转换/压缩)
音频: _Sound/GN_Sound → GN_Audio(音乐控制)
2D:   GN_Color → GN_Layout(位置布局/排版/绘制坐标) → GN_2Dmath → GN_Linear → GN_2DArea
       → GN_2Dtype → _Graphics/GN_G2D(绘制/帧缓冲) → GN_2D(复合图形/截图/比例/画中画)
       → GN_Input(键鼠触/输入记录回放/计时器) → GN_Init(运行流程/更新/ini) → _Main/GN_Main(窗口/主循环)
       └ Utility: GN_InputAct → GN_Font(高速文字) → GN_RunRec(运行记录)
         → GN_AmCtrl(动画时间) → GN_Frame(插值动画/粒子/动态文字)
         → GN_2Dscene(2D 对象/场景二进制) → GN_UI(界面组件/UI 场景读写)
         → GN_UiBase(简单组件单元) → UiListGui(嵌套滚动筐) → UiListTree(列表树)
         → UiTextIn(文本输入框) → UiFBrowser(文件浏览) → UiPopDlg(弹出选单/编辑器插件接口)
3D:   GN_3Dmath → GN_3DArea → GN_3Dtype → GN_3DMesh → _Graphics/GN_G3D(渲染对象/场景渲染/烘焙)
       └ Utility: GN_3DAmKey(动画关键点) → GN_3DMeshEdit → GN_3D(模型/灯光/摄像机)
         → GN_3Dscene(3D 场景读写)
网络: GN_NetBase → _Net/GN_Net → NetTrans(HTTP/下载/异步多任务服务器) → NetFile(网络文件当本地读)
```

（行锚：`GN.h:147` Layout、`:152` G2D、`:159` Font、`:164` UI、`:180` G3D、`:184` GN_3D。）

## 命名法与"实用性"标注（代码功能简述与规范.txt 的约定）

- 全局函数/变量：大写开头驼峰；结构体 `sXXX`、类 `cXXX`（"通常带有 ★ 越多的代表实用性越高"）、
  常量全大写、枚举 `eXXX`（2 的倍数可 `|` 组合）；局部变量全小写下划线、`_` 前缀为裸指针。
- `z??????` 前缀=内部接口、"不建议使用"（新版本可能移除）；`a??????` 前缀=**交给用户直接设置的属性**；
  `________` 全小写下划线=结构体属性。
- 初始化约定：`.Init` 只设初值不分配；函数名含 `ToData`/`Data` 的会分配动态内存；用毕 `.Release`。

## DLL 分工（导出计数见 README 复算）

| DLL | 导出数 | 角色（按头文件索引与依赖树） |
| --- | --- | --- |
| `GNwin32.dll` | 4383 | 主类库（GN/S/GNf/GNg/GNu/GN3d 全部命名空间的门面） |
| `GN_Gwin32.dll` | 214 | G2D/G3D 图形底件（`GT_*` 转换层 + `GNg::` 对外接口） |
| `GN_Sys_win32.dll` | 69 | 系统（IO/线程/时间/转码） |
| `GN_Edi_win32.dll` | 40 | 编辑器/场景编辑（`GN_Edi.h`、`GN_EdPlugin.h`） |
| `GN_Sound_win32.dll` | 37 | 声音 |
| `GN_Media_win32.dll` | 27 | 视频/多媒体（`GN_Media.h`） |
| `GN_Main_win32.dll` | 22 | 程序入口/窗口/主循环（`_Main/GN_Main.h`） |
| `GN_Net_win32.dll` | 4 | 网络底件 |

## 样例导读（`Sample/base/基本例子说明.txt`，23 例 + 3D 10 例）

- 框架：`Sample1.cpp` 展示应用入口协议——引擎每帧回调四个 `GN_CALL` 函数：
  `AppInit` / `AppDisplay2D` / `AppDisplay3D` / `AppRun` / `AppUnInit`（`Sample1.cpp:3`–`:20`；首个 2D 例
  用 `GN_RunTextOut` 直接输出 "Hello World"）。
- 2D 例覆盖：图筐/线筐、图片缩放与动态改像素、插值、安全指针、`cFRAME` 动画筐（分格/声音事件/分段）、
  粒子发射器、`cTEXT_ART` 文本（**字体图由 Photoshop 模板生成**）、音视频与滚动字幕、文件读写、
  `cSIM_DIALOG` 设置对话框、**文本输入筐**、多视口画中画、滚动区、滚动图标/文本列表、会话气泡、
  触屏虚拟摇杆/按钮。
- 3D 例（`Sample/3d/`）：场景读入/相机、**光追全局光照 + HDR 自适应曝光**、对象控制与碰撞、
  骨骼动画与角色操控（RTS/MOBA 与魂类越肩两套）、一个完整小游戏（地形+物理+地图绘制）。

## 一个真实的运行期观察（`RunLog.txt`，GBK 编码）

日志第 1 行就是一次资源失败："读取图象文件失败! 找不到文件 : `Sample/ui/LoginUI/GB2312_system_`"
（`RunLog.txt:1`；**该文件是 GBK 编码**——本项目文档统一 UTF-8，引用时保留原字节串）。
随后是运行时 UI 场景树的逐节点转储（`RunLog.txt:2` 起）：`GN- edit ui scene cUI_GROUP 0` →
`bg cUI_IMAGE 1` → `dlg_login cUI_GROUP 2` → … → `in_user cUI_TEXT_INPUT 9` ——
这就是 `cUI_SCENE::PrintAllNodeClassName()`（`GN_UI.h:351`）那类调试输出的现场，说明 SDK 自带
"场景树可转储 + 资源缺失可定位"的诊断面（**与"字体图缺失"强相关的失败模式**，见 02 篇）。
