# Codex Meter

一个用 Rust + Tauri 构建的 Codex 菜单栏/系统托盘小组件。

当前阶段以测试体验为主，不发布 Release。

## 交互方式

### 菜单栏模式（默认）

- macOS：常驻顶部菜单栏，不显示 Dock 图标
- Windows：常驻系统托盘，不显示任务栏按钮
- 单击仪表图标打开用量面板
- 点击面板外部自动收起
- 面板右上角的收起按钮不会退出应用
- 右键图标可以刷新、打开设置、切换模式或退出

### 贴在桌面

- 窗口保持在普通应用窗口下方
- 不占 Dock 或任务栏
- 可以拖动位置并自动记忆
- 仍可通过菜单栏/托盘图标暂时隐藏和找回

## 数据来源

- 计划、用量窗口、剩余百分比和重置时间：Codex 官方本地 `app-server`
- 每日、累计和最近任务 Token：Codex 官方本地数据
- 任务活动状态：只读分析 Codex 生成的本地任务事件
- 续费日期：用户本机设置，因为 Codex 本地协议目前不提供该字段

应用不会读取或上传登录令牌、邮箱和对话正文。

## 本机开发

### 常规环境

需要：

- Node.js 20+
- Rust stable 1.77.2+
- macOS Command Line Tools，或 Windows C++ Build Tools
- 已安装并登录的 ChatGPT/Codex 桌面端或 Codex CLI

```bash
npm install
npm run dev
```

### 当前测试机

当前 macOS 的系统 Command Line Tools 目录不完整，因此项目脚本会自动使用安装在以下用户目录的独立 Apple 工具链：

```text
~/.local/share/codex-meter-clt-root/Payload/Library/Developer/CommandLineTools
```

不需要修改系统目录或输入管理员密码。也可以通过 `CODEX_METER_CLT_DIR` 指定其他路径。

## 测试与构建

```bash
npm test
npm run build
```

当前先以本机测试为主。确定第一版交互后，再启用 macOS/Windows CI、Release 与签名发布流程。
