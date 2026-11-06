# 鼠标灵敏度配置切换工具

Rust 编写的 Windows 10/11 x64 常驻工具。每只可识别的鼠标保存独立指针速度和“提高指针精确度”，移动、点击或滚动哪只鼠标，就自动应用它的配置。无需管理员权限。

## 使用

运行 `target/release/mouse-speed-switcher.exe`，在窗口左侧选择鼠标配置，设置名称、1–20 的速度及精确度开关，点击“保存配置”。当前活动鼠标的修改立即生效；其他鼠标在下次操作或接入时生效。“立即应用已保存配置”不会保存表单中的未保存修改。

- ● 表示此配置有关联且已连接的设备；○ 表示历史配置。状态区显示当前活动配置。
- 两只鼠标同时连接也能切换，包括接收器长期插着的情况。静止报告不会触发切换。
- 新设备复制程序本次启动前的系统参数。启动时只连接一只鼠标则直接加载，多只则等待操作。
- 换端口后如出现新配置，在左侧选择原有配置，再在下面的设备下拉框中选择新识别的鼠标，点击“关联到选中配置”。原配置及历史记录保留。
- 编辑中的表单不随活动鼠标改变；切换左侧选择会载入该项已保存值。请先保存再切换选择。
- 关闭设置窗口后继续在托盘运行；双击托盘图标重新打开，右键可暂停、设置自启或退出。
- 暂停时不自动切换；“立即应用”以及保存当前活动配置仍可主动应用参数。
- 自启默认关闭。启用后以 `--background` 启动，启动项位于当前用户的 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\MouseSpeedSwitcher`。移动 EXE 后请关闭并重新启用自启。
- 再次运行会打开已有设置窗口，不会启动多个监听器。

正常退出时，如果系统参数仍等于工具最后成功应用的值，会恢复启动前参数；如果其他软件已改变参数则保留其修改。强制结束进程、崩溃或断电无法保证恢复。

## 配置与恢复

配置位于 `%LOCALAPPDATA%\MouseSpeedSwitcher\config.json`。使用带版本号的 JSON，包含设备身份、别名、速度、加速参数及设备到配置的关联；不包含输入历史或鼠标轨迹，无网络服务。

保存使用同目录临时文件、刷盘及原子替换；上一个有效版本保存为 `config.json.bak`。读取损坏配置时先保留为 `config.corrupt-时间戳.json`，再尝试备份，否则创建空配置，并在状态区提示。恢复更多配置前请退出工具再修改文件。首版仅支持 `version: 1`；更高版本不会直接被当作有效配置覆盖。

身份优先采用 Windows Container ID，回退为设备实例 ID；接口路径、实例、可用 USB 序列号一并记录，便于诊断。同型号设备不会仅按 VID/PID 合并。某些接收器将多只实体鼠标暴露为同一个设备，这时系统无法区分它们，只能共用配置。没有稳定标识的设备换端口可能需要重新关联。无法解析身份的设备、远程桌面鼠标及没有来源句柄的输入不会自动创建配置。

## 工作原理与限制

后台专用线程通过 Raw Input 接收设备来源及插拔事件，不拦截输入、不模拟移动、不修改硬件 DPI、不安装驱动。UI 独立运行，输入热路径只访问缓存；同一配置不重复写系统参数，设备发现或显式保存时才写文件。失败重试有节流，错误显示在窗口状态区。

Windows 的速度和精确度是**全局指针参数**。本工具自动切换这些参数，不是为同时移动的两只鼠标分别变换每一个输入包；触发切换的首个输入可能仍使用旧参数。两只鼠标真正同时移动时以事件到达顺序切换。直接消费 Raw Input 的游戏可能忽略系统指针参数。精确触控板可能没有设备句柄，不保证支持。

速度采用 Windows 原生的 1–20 刻度。精确度对应 `SPI_GETMOUSE/SPI_SETMOUSE` 的加速状态；保存已开启状态会保留原有完整参数，从关闭改为开启时使用标准 `[6,10,1]`，关闭时保留阈值并将加速状态设为 0。系统切换不反复持久化 Windows 注册表参数。高级自定义加速曲线不在首版配置范围内。

API 参考：[Raw Input 注册](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerrawinputdevices)、[设备插拔消息](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-input-device-change)、[系统参数](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)。

## 构建

推荐安装 Rust stable MSVC 工具链和 Visual Studio Build Tools 的“使用 C++ 的桌面开发”（含 Windows SDK），从 Developer PowerShell 执行：

```powershell
cargo fetch --locked
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

或依赖已缓存后执行 `./build.ps1 -Task All`，自动完成格式检查、Clippy、单元测试和 release 构建。支持 `Check`、`Test`、`Build`。输出为 `target/release/mouse-speed-switcher.exe`，release 无控制台窗口。

当前机器缺少 MSVC SDK，构建脚本也支持本地 `target/tooling/winsysroot` 中由 [xwin](https://github.com/Jake-Shadle/xwin) 获取的 Microsoft CRT/SDK 库，并使用 Rust 自带的 `rust-lld`，无需系统安装。此目录为构建依赖，不需随 EXE 分发。项目静态链接 VC 运行库。可重建该依赖：

```powershell
# 使用 xwin 0.10.0，先从其正式发布下载并核验 SHA256；阅读并接受 Microsoft SDK 许可。
xwin --accept-license --cache-dir target/tooling/xwin-cache splat --disable-symlinks --use-winsysroot-style --preserve-ms-arch-notation --output target/tooling/winsysroot
./build.ps1 -Task All
```

`--diagnose` 只枚举设备、读取系统参数并向标准输出写 JSON，不启动监听器、不修改速度、不创建配置或自启项。适合在终端中诊断识别问题。

本地 SDK 未下载 Microsoft CRT 调试符号时，调试/测试链接可能提示 LNK4099（找不到 Microsoft 自身的 PDB）；不影响运行和 Rust 代码调试。发布版本已成功构建。

## 验证

自动测试使用模拟系统接口，不修改真实鼠标设置，覆盖高频输入去重、参数写入失败、暂停及主动应用、退出恢复与外部修改、身份区分、新设备默认值及配置恢复。

实机验收请检查：

1. USB A 设置慢速且关闭精确度，B 设置快速且开启精确度；交替移动、点击、滚轮，检查状态及 Windows 鼠标控制面板参数。
2. 拔插 A/B、蓝牙断连重连、接收器常插、启动时多设备、最后一只设备拔出。
3. 换 USB 端口、同型号不同设备、重新关联后重启；确认历史配置保留。
4. 编辑表单期间操作另一只鼠标，确认表单不被覆盖；确认托盘暂停、恢复、重复启动和自启。
5. 正常退出恢复参数；使用 Windows 控制面板改动后退出，确认不覆盖外部设置。
6. 高回报率鼠标持续移动，确认不卡顿、同一配置不重复调用设置 API 或写文件。

实体双鼠标、蓝牙及高回报率硬件的实测结果以 `VALIDATION.md` 为准。
