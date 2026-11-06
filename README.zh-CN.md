# Mouse Speed Switcher _(mouse-speed-switcher)_

为每只鼠标保存独立的指针速度和“提高指针精确度”，操作时自动切换的 Windows 常驻工具。

适合在办公鼠标、游戏鼠标等多只设备之间切换。移动、点击或滚动某只可识别的鼠标，即可应用它的已保存配置，无需反复打开 Windows 鼠标设置。

使用 Rust 编写，面向 **Windows 10/11 x64**，无需管理员权限、额外驱动或网络服务。当前版本为 **1.1.7**。产品和项目目录使用名称 Mouse Speed Switcher，Cargo 包名使用对应的连字符形式 `mouse-speed-switcher`。

项目仓库：[Edward7x/mouse-speed-switcher](https://github.com/Edward7x/mouse-speed-switcher)。

[English](README.md) | **简体中文**

## 目录

- [背景](#背景)
- [安装](#安装)
- [使用](#使用)
- [配置与恢复](#配置与恢复)
- [工作原理与限制](#工作原理与限制)
- [开发与构建](#开发与构建)
- [验证](#验证)
- [贡献](#贡献)
- [许可证](#许可证)

## 背景

Windows 的指针速度和“提高指针精确度”是全局设置，多只鼠标默认共用同一组参数。本工具根据输入事件识别正在使用的设备，再切换到对应配置。

- 每个配置分别保存名称、1–20 的指针速度及精确度状态。
- 支持多只鼠标同时连接，包括无线接收器一直插着的情况；静止报告不会触发切换。
- 支持设备重新关联、托盘暂停、登录自启和单实例运行。
- 正常退出时有条件地恢复启动前的系统参数，避免覆盖其他软件后续作出的修改。

## 安装

### 依赖

运行已构建的 EXE 需要 Windows 10/11 x64。程序静态链接 VC 运行库，无需安装 Rust；从源码构建所需的工具见[开发与构建](#开发与构建)。其他 Windows 机器的兼容性仍待实机验收。

### 使用

当前应用界面使用中文标签；英文文档保留对应的中文按钮名，便于对照操作。本地发布包

当前工作区已生成 `dist/MouseSpeedSwitcher-1.1.7-windows-x64.zip`，包含正式版、界面预览版、中英文 README、MIT LICENSE 和验证记录。从项目根目录执行：

```powershell
Expand-Archive -LiteralPath .\dist\MouseSpeedSwitcher-1.1.7-windows-x64.zip -DestinationPath .\MouseSpeedSwitcher-1.1.7
Set-Location .\MouseSpeedSwitcher-1.1.7
.\mouse-speed-switcher.exe
```

也可直接运行 `dist/1.1.7/mouse-speed-switcher.exe`。这是本地构建产物的位置，仓库未配置公开下载地址。

### 更新与移除

更新前从旧版托盘菜单选择“退出”，再运行新版。单实例机制会激活已有进程，不会自动替换运行中的旧版。

如果启用了自启，移动或替换 EXE 后，请关闭并重新启用自启以更新路径。

移除时先关闭自启，再从托盘退出，最后删除程序文件。配置默认保留在 `%LOCALAPPDATA%\MouseSpeedSwitcher`；不再需要时可自行删除该目录。

## 使用

当前应用界面使用中文标签；英文文档保留对应的中文按钮名，便于对照操作。

### 保存第一份配置

1. 启动程序，在左侧选择鼠标配置。
2. 设置名称、指针速度和“提高指针精确度”，点击“保存配置”。
3. 操作另一只鼠标，为它保存所需的参数。
4. 交替移动、点击或滚动鼠标，查看状态区中的当前活动配置。

当前活动鼠标的配置保存后立即生效；其他鼠标的配置在下次操作或接入时生效。新设备初始值复制自程序本次启动前的系统参数。启动时仅连接一只可识别鼠标会直接加载配置，多只则等待操作。

### 编辑与设备关联

| 界面项目 | 行为 |
| --- | --- |
| 左侧配置列表 | 优先显示已连接配置；● 表示有关联且已连接的设备，○ 表示历史配置。 |
| 当前活动配置 | 显示正在使用的鼠标，与左侧正在编辑的配置分别呈现。 |
| 保存配置 | 保存表单；若编辑的是当前活动配置，同时应用参数。 |
| 立即应用已保存配置 | 应用已保存值，不保存表单中尚未保存的修改。 |
| 关联到选中配置 | 将下拉框中的设备关联到左侧选中的配置，保留原配置及历史记录。 |

活动鼠标变化不会覆盖正在编辑的表单；切换左侧选择会载入该项已保存值，因此请先保存修改再切换选择。

换 USB 端口后如出现新配置，先选择原有配置，再从设备下拉框选择新识别的鼠标，点击“关联到选中配置”。设备关联及身份详情可展开查看。

### 托盘与自启

- 关闭正式版设置窗口后，程序继续在托盘运行；双击托盘图标重新打开窗口。
- 右键托盘图标可暂停或恢复自动切换、设置自启或退出。
- 暂停仅停止自动切换；“立即应用”及保存当前活动配置仍可主动应用参数。
- 自启默认关闭。启用后通过当前用户注册表项 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\MouseSpeedSwitcher`，以 `--background` 启动。
- 再次运行正式版会打开已有设置窗口，不会启动多个监听器。

### 命令行

以下示例在 EXE 所在目录执行：

```powershell
# 正常启动并显示设置窗口
.\mouse-speed-switcher.exe

# 后台启动到托盘
.\mouse-speed-switcher.exe --background

# 使用示例设备预览界面
.\mouse-speed-switcher.exe --preview

# 预览无配置状态
.\mouse-speed-switcher.exe --preview --preview-empty

# 只读诊断：向标准输出写出设备和当前系统参数的 JSON
.\mouse-speed-switcher.exe --diagnose
```

预览模式不启动输入监听、不写用户配置、不修改系统参数或启动项。保存、应用、关联、暂停和自启仅改变内存中的演示状态；关闭预览窗口即退出。预览包含托盘图标和右键菜单。

发布包中的 `mouse-speed-switcher-ui-preview.exe` 自动进入预览模式。若依赖文件名识别模式，需保留 `-preview` 后缀；也可显式传入 `--preview`。

`--diagnose` 不启动监听器，也不创建配置或自启项。诊断输出包含设备身份信息；分享前可按需隐去序列号和设备路径。

## 配置与恢复

配置文件位于：

```text
%LOCALAPPDATA%\MouseSpeedSwitcher\config.json
```

JSON 配置包含设备身份、别名、速度、完整加速参数及设备到配置的关联，不记录输入历史或鼠标轨迹。当前支持的配置格式版本为 `version: 1`。

保存时使用同目录临时文件、刷盘及原子替换，并将上一个有效版本保存为 `config.json.bak`。读取无效配置时，会先保留为 `config.corrupt-时间戳.json`，再尝试载入备份；备份不可用则使用空配置，并在状态区提示。更高版本的配置不会被直接视为有效配置，原文件也会先保留。

手动恢复或修改配置前，请先退出程序。

正常退出时，若系统参数仍等于工具最后成功应用的值，会恢复启动前的参数；若其他软件已改变参数，则保留其修改。强制结束进程、崩溃或断电时无法保证恢复。

## 工作原理与限制

后台线程通过 Windows Raw Input 接收设备来源和插拔事件，不拦截输入、不模拟移动、不修改硬件 DPI。界面独立运行，输入热路径访问缓存；同一配置不重复写系统参数，设备发现或显式保存时才写文件。失败重试有节流，错误显示在窗口状态区。

### 设备识别

身份优先采用 Windows Container ID，回退为设备实例 ID；同时记录接口路径、实例及可用的 USB 序列号，便于诊断。同型号设备不会仅按 VID/PID 合并。

- 某些接收器将多只实体鼠标暴露为同一个设备，此时只能共用配置。
- 没有稳定标识的设备换端口后可能需要重新关联。
- 无法解析身份的设备、远程桌面鼠标及没有来源句柄的输入不会自动创建配置。
- 精确触控板可能没有设备句柄，不保证支持。

### 系统参数

工具切换的是 Windows 的**全局指针参数**。触发切换的首个输入可能仍使用旧参数；两只鼠标同时移动时，按事件到达顺序切换。直接消费 Raw Input 的游戏可能忽略这些系统设置。

速度使用 Windows 原生的 1–20 刻度。精确度对应 `SPI_GETMOUSE` / `SPI_SETMOUSE` 的加速状态：保存已开启状态会保留完整参数，从关闭改为开启时使用标准 `[6, 10, 1]`，关闭时保留阈值并将加速状态设为 0。切换时不反复持久化 Windows 注册表参数，当前不提供高级自定义加速曲线配置。

## 开发与构建

### 构建依赖

- Rust stable，使用 `x86_64-pc-windows-msvc` 工具链并支持 Rust 2024 edition。
- Visual Studio Build Tools 的“使用 C++ 的桌面开发”组件，包含 Windows SDK。

从项目根目录，在 Developer PowerShell 中执行：

```powershell
cargo fetch --locked
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

依赖已缓存后，也可运行离线构建脚本：

```powershell
.\build.ps1 -Task All
```

`All` 依次执行格式检查、Clippy、测试和 release 构建；也支持 `Check`、`Test`、`Build`。输出为 `target/release/mouse-speed-switcher.exe`，release 版本不显示控制台窗口。

### 可选的本地 SDK

构建脚本也支持 `target/tooling/winsysroot` 中的 Microsoft CRT/SDK，并使用 Rust 自带的 `rust-lld`。该目录仅为构建依赖，无需随 EXE 分发。

本地验证使用 [xwin](https://github.com/Jake-Shadle/xwin) 0.10.0 准备此目录。若采用该方案，先获取并核验 xwin 的 SHA256，阅读并接受 Microsoft SDK 许可，再执行：

```powershell
xwin --accept-license --cache-dir target/tooling/xwin-cache splat --disable-symlinks --use-winsysroot-style --preserve-ms-arch-notation --output target/tooling/winsysroot
.\build.ps1 -Task All
```

未下载 Microsoft CRT 调试符号时，调试或测试链接可能提示 LNK4099；现有验证记录中该提示未导致构建失败，详见[验证记录](VALIDATION.md)。

### 界面与图标

界面采用浅色双栏布局，内容区可滚动，底部保存和运行操作保持可见。本地设计原图与功能约定位于 `design/ui-v1/`，实现对照见 `design/ui-v1/IMPLEMENTATION.md`；这些设计资料不纳入 Git 仓库。

图标使用蓝紫双鼠标设计。矢量源文件为 [assets/app-icon.svg](assets/app-icon.svg)，ICO 包含 16–256px 的九个尺寸，用于 EXE、设置窗口和托盘；`build.rs` 自动嵌入资源，无需额外资源编译器。

修改矢量设计时，需要同步更新 `scripts/export-icon.py` 中的几何形状。安装 Python 和 Pillow 后执行：

```powershell
python -m pip install Pillow
python scripts/export-icon.py
```

## 验证

自动测试使用模拟系统参数接口及隐藏原生控件，覆盖输入去重、写入失败、暂停和主动应用、退出恢复、设备身份、配置关联与损坏恢复，以及界面和托盘回归；不会修改真实鼠标设置。

[VALIDATION.md](VALIDATION.md) 记录了 1.1.7 的格式检查、Clippy、14 项测试及 release 构建通过结果，以及历史验证与待验收项。该记录不代表所有硬件场景均已通过实机验收。

实机验证应覆盖：

1. 两只鼠标交替移动、点击和滚轮，核对 Windows 中的实际速度及精确度。
2. USB 拔插、蓝牙重连、接收器常插、启动时多设备和最后一只设备拔出。
3. 换端口、同型号不同设备、重新关联及重启后的配置保留。
4. 编辑表单时切换活动鼠标，以及托盘暂停、重复启动和登录自启。
5. 正常退出恢复参数，以及外部软件改动参数后的保留。
6. 高回报率鼠标的响应和资源开销，以及不同 DPI 和 Windows 机器的兼容性。

## 贡献

有仓库访问权限的协作者可通过 [Issues](https://github.com/Edward7x/mouse-speed-switcher/issues) 反馈问题或提出建议，通过 [Pull requests](https://github.com/Edward7x/mouse-speed-switcher/pulls) 提交改进。暂未获得访问权限时，可通过当前项目协作渠道反馈。

反馈问题时，请附上 Windows 版本、程序版本、设备及连接方式、复现步骤和预期行为。识别问题可附脱敏后的 `--diagnose` 输出。

提交代码修改前运行 `build.ps1 -Task All`，并在涉及设备切换、系统参数、托盘或自启时说明实机验证结果。构建和自动测试通过不能替代硬件验收。

## 许可证

本项目采用 [MIT License](LICENSE)。Copyright (c) 2026 Eddie7x。

允许使用、修改、分发及商业使用，分发时须保留版权和许可声明。完整条款及免责声明见许可证文件；第三方依赖遵循各自的许可证。
