# Codex Meter

一个用 Rust + Tauri 构建的 Codex 菜单栏/系统托盘小组件，用紧凑的仪表盘显示用量、重置时间、任务状态和 Token 统计。

[下载最新版本](https://github.com/zc-xu/codex-meter/releases/latest)

## 功能

### 菜单栏模式（默认）

- macOS 常驻顶部菜单栏，不显示 Dock 图标
- Windows 常驻系统托盘，不显示任务栏按钮
- 单击仪表图标打开；再次单击图标或点击收起按钮后隐藏
- 菜单栏/托盘圆环随剩余额度顺时针变化，可以快速判断当前用量
- 右键图标可以刷新、打开设置、切换模式或退出

### 贴在桌面

- 窗口保持在普通应用窗口下方，不遮挡工作
- 可以拖动并自动记忆位置
- 自动适配 Retina、多显示器和屏幕可用区域
- 仍可通过菜单栏/托盘图标隐藏和找回

### 数据

- 套餐、用量窗口、剩余百分比与重置时间
- 今日、近 7 日、累计及当前任务 Token
- 任务运行、等待输入、等待审批与空闲状态
- 记录上次充值日期，并按月自动推算下一次续费时间

## 安装

在 [Releases](https://github.com/zc-xu/codex-meter/releases) 下载对应平台的安装包。

- macOS：支持 macOS 12 及以上。当前开源构建使用 ad-hoc 签名，未经过 Apple 公证；首次打开可能需要在“系统设置 → 隐私与安全性”中允许。
- Windows：安装包暂未购买商业代码签名证书，首次运行可能出现 SmartScreen 提示。

需要已安装并登录的 ChatGPT/Codex 桌面端或 Codex CLI。

## 隐私

Codex Meter 在本机运行，没有遥测或独立的上传服务。它通过 Codex 本地 `app-server` 获取用量，并读取本地任务事件来计算 Token 与状态。

更精确的数据访问范围、存储内容和限制见 [PRIVACY.md](PRIVACY.md)。

设置中的“Codex 程序路径”是故障排查用的备用项。连接正常时应保持为空；只有自动查找失败时才需要填写 `codex` 或 `codex.exe` 的完整路径。

## 开发

需要 Node.js 20+、Rust stable 1.77.2+，以及 macOS Command Line Tools 或 Windows C++ Build Tools。

```bash
npm install
npm run dev
```

测试与构建：

```bash
npm test
npm run build
```

项目脚本会优先使用系统工具链。在系统 Command Line Tools 不完整时，也支持通过 `CODEX_METER_CLT_DIR` 指定其他 Apple 工具链目录。

## License

[MIT](LICENSE)
