---
version: 0.3
name: Daybook-desktop-tokens
description: 日簿 Daybook 桌面端的 token design system —— 一套为「事后补记」写的全亮界面。底色是 Cloud Dancer 米白 ({colors.paper-200})，也正是应用图标的底板；棕墨 ({colors.ink-950}) 只作为文字、图标与线，层次由四级浅面加一根发丝线承担，不靠明暗反转。四个语义色同亮度 (L .58) 同彩度 (C .105) 站在一个色环上——靛青 action、松绿 settled、麻黄 caution、朱砂 alarm，全部取自应用图标里已有的颜色。三层单向依赖：primitive → semantic（按区域重映射）→ component，组件只准引用 semantic。产品最生死的一条约束在 token 层就有名字：AI 写入的草稿只给中性灰与 45° 斜纹，机器写的东西不配拥有颜色。

colors:
  # ── 签名色别名：给读 design.md 的生成器认的「主色」，等同 brand-600 / paper-50
  primary: "#1f5f8f"
  on-primary: "#fdfcf9"
  # ── L1 primitive · ink（暖调棕墨 hue 55，只负责文字、图标与线）
  ink-950: "#211208"
  ink-900: "#311f13"
  ink-800: "#463021"
  ink-700: "#5a493e"
  ink-600: "#73665d"
  ink-500: "#8f847c"
  ink-400: "#aca29c"
  ink-300: "#cbc2bc"
  ink-200: "#e0d9d5"
  ink-100: "#f1ebe8"
  # ── L1 primitive · paper（棉纸 hue 82，负责所有纸面）
  paper-50: "#fdfcf9"
  paper-100: "#f7f5f1"
  paper-200: "#f0ede8"
  paper-300: "#e8e4dd"
  paper-400: "#ded8ce"
  paper-500: "#c1b6a2"
  # ── L1 primitive · 四个语义色，同 L .58 同 C .105，只换 hue
  critical-100: "#fdece8"
  critical-200: "#f3c4bc"
  critical-500: "#b06154"
  critical-600: "#8a4338"
  critical-700: "#663028"
  caution-100: "#f7efe2"
  caution-200: "#e4cea9"
  caution-500: "#9b7225"
  caution-600: "#785300"
  caution-700: "#583d00"
  positive-100: "#e7f4e9"
  positive-200: "#b8dcbe"
  positive-500: "#488c58"
  positive-600: "#296a3b"
  positive-700: "#1d4e2b"
  brand-100: "#e6f2fd"
  brand-200: "#b4d6f3"
  brand-500: "#3e80b4"
  brand-600: "#1f5f8f"
  brand-700: "#15466a"

  # ── L2 semantic · 文字（rail 与 paper 两个区域完全相同，区域只换面色不换字色）
  fg-primary: "#211208"
  fg-secondary: "#5a493e"
  fg-muted: "#73665d"
  fg-faint: "#8f847c"
  fg-numeral: "#583d00"
  # ── L2 semantic · data-region="paper"（灯箱 · 审核栏 · 账目 · 设置 · 文档）
  paper-bg-canvas: "#e8e4dd"
  paper-bg-surface: "#f7f5f1"
  paper-bg-surface-raised: "#fdfcf9"
  paper-bg-surface-sunken: "#e8e4dd"
  paper-bg-field: "#fdfcf9"
  paper-bg-selected: "#e6f2fd"
  paper-bg-hover: "rgba(33,18,8,.04)"
  paper-bg-pressed: "rgba(33,18,8,.08)"
  paper-bg-disabled: "#c1b6a2"
  paper-border-subtle: "rgba(33,18,8,.10)"
  paper-border-default: "rgba(33,18,8,.18)"
  paper-border-strong: "rgba(33,18,8,.45)"
  # ── L2 semantic · data-region="rail"（顶栏 · 来源夹 · 侧栏）
  rail-bg-canvas: "#e8e4dd"
  rail-bg-surface: "#f0ede8"
  rail-bg-surface-raised: "#fdfcf9"
  rail-bg-surface-sunken: "#e8e4dd"
  rail-bg-field: "#fdfcf9"
  rail-bg-selected: "#fdfcf9"
  rail-bg-hover: "rgba(33,18,8,.035)"
  rail-bg-pressed: "rgba(33,18,8,.07)"
  rail-bg-disabled: "#ded8ce"
  rail-border-subtle: "rgba(33,18,8,.10)"
  rail-border-default: "rgba(33,18,8,.16)"

  # ── L2 semantic · intent（不随区域变化：差额报警在哪都是同一个红）
  #    accent=500 只做图标 / 进度 / 色条 / 描边，永不承载文字
  #    fill=600 是「能压白字的实底」，fill-hover=700
  intent-settled-surface: "#e7f4e9"
  intent-settled-border: "#b8dcbe"
  intent-settled-accent: "#488c58"
  intent-settled-fill: "#296a3b"
  intent-settled-fill-hover: "#1d4e2b"
  intent-settled-text: "#1d4e2b"
  intent-pending-surface: "#f7efe2"
  intent-pending-border: "#e4cea9"
  intent-pending-accent: "#9b7225"
  intent-pending-fill: "#785300"
  intent-pending-fill-hover: "#583d00"
  intent-pending-text: "#583d00"
  intent-alarm-surface: "#fdece8"
  intent-alarm-border: "#f3c4bc"
  intent-alarm-accent: "#b06154"
  intent-alarm-fill: "#8a4338"
  intent-alarm-fill-hover: "#663028"
  intent-alarm-text: "#663028"
  intent-action-surface: "#e6f2fd"
  intent-action-border: "#b4d6f3"
  intent-action-accent: "#3e80b4"
  intent-action-fill: "#1f5f8f"
  intent-action-fill-hover: "#15466a"
  intent-action-text: "#15466a"
  intent-on-fill: "#fdfcf9"

  # ── L2 semantic · 领域（Daybook 独有：证据 / 草稿 / 账目）
  evidence-sheet: "#fdfcf9"
  evidence-desk: "#e8e4dd"
  evidence-rule: "#e4cea9"
  evidence-quote-bg: "#e8e4dd"
  draft-surface: "rgba(253,252,249,.85)"
  draft-marker-picked: "#1f5f8f"
  draft-marker-written: "#aca29c"
  draft-border-gap: "rgba(176,97,84,.5)"
  ledger-fact: "#211208"
  ledger-expense: "#211208"
  ledger-income: "#1d4e2b"

  # ── L2 semantic · 图表色序（为 M1 统计图预留；1–4 与意图色同值）
  chart-1: "#488c58"
  chart-2: "#3e80b4"
  chart-3: "#9b7225"
  chart-4: "#b06154"
  chart-5: "#68b4b3"
  chart-6: "#ac9acd"
  chart-7: "#9eac76"
  chart-8: "#cf9295"

typography:
  display-lg:
    fontFamily: Noto Serif SC
    fontSize: 46px
    fontWeight: 600
    lineHeight: 1.24
    letterSpacing: -0.005em
  display-md:
    fontFamily: Noto Serif SC
    fontSize: 36px
    fontWeight: 600
    lineHeight: 1.26
  title-lg:
    fontFamily: Noto Serif SC
    fontSize: 27px
    fontWeight: 600
    lineHeight: 1.3
  title-md:
    fontFamily: Noto Serif SC
    fontSize: 21px
    fontWeight: 600
    lineHeight: 1.35
  title-sm:
    fontFamily: Noto Serif SC
    fontSize: 17px
    fontWeight: 600
    lineHeight: 1.4
  body-lg:
    fontFamily: IBM Plex Sans
    fontSize: 15px
    fontWeight: 400
    lineHeight: 1.6
  body-md:
    fontFamily: IBM Plex Sans
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.6
  body-sm:
    fontFamily: IBM Plex Sans
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.55
  label:
    fontFamily: IBM Plex Sans
    fontSize: 11px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.02em
  eyebrow:
    fontFamily: IBM Plex Mono
    fontSize: 11px
    fontWeight: 600
    lineHeight: 1
    letterSpacing: 0.16em
  serif-quote:
    fontFamily: Noto Serif SC
    fontSize: 15px
    fontWeight: 400
    lineHeight: 1.9
  money-lg:
    fontFamily: IBM Plex Mono
    fontSize: 28px
    fontWeight: 600
    lineHeight: 1.1
  money-md:
    fontFamily: IBM Plex Mono
    fontSize: 20px
    fontWeight: 600
    lineHeight: 1.2
  money-sm:
    fontFamily: IBM Plex Mono
    fontSize: 15px
    fontWeight: 600
    lineHeight: 1.2
  num-meta:
    fontFamily: IBM Plex Mono
    fontSize: 11px
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: 0.04em

rounded:
  none: 0px
  sm: 6px
  md: 10px
  lg: 14px
  full: 999px

spacing:
  px: 1px
  half: 2px
  1: 4px
  2: 8px
  3: 12px
  4: 16px
  5: 20px
  6: 24px
  8: 32px
  10: 40px
  12: 48px
  16: 64px
  rail-width: 252px
  tray-min: 390px
  tray-max: 460px
  sheet-max: 780px

components:
  # ── 按钮：一屏只允许一个 primary；破坏性动作永远 ghost
  button-primary:
    backgroundColor: "{colors.intent-action-fill}"
    textColor: "{colors.intent-on-fill}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "0 {spacing.3}"
    height: 32px
  button-primary-hover:
    backgroundColor: "{colors.intent-action-fill-hover}"
    textColor: "{colors.intent-on-fill}"
  button-primary-lg:
    backgroundColor: "{colors.intent-action-fill}"
    textColor: "{colors.intent-on-fill}"
    rounded: "{rounded.sm}"
    padding: "0 {spacing.4}"
    height: 40px
  button-primary-disabled:
    backgroundColor: "{colors.paper-bg-disabled}"
    textColor: "{colors.fg-faint}"
    rounded: "{rounded.sm}"
  button-secondary:
    backgroundColor: "{colors.paper-bg-surface-raised}"
    textColor: "{colors.fg-primary}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "0 {spacing.3}"
    height: 32px
    border: "1px solid {colors.paper-border-default}"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.intent-action-text}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "0 {spacing.2}"
    height: 26px
  button-alarm-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.intent-alarm-text}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "0 {spacing.2}"
    height: 26px
  button-focus-ring:
    shadow: "0 0 0 2px {colors.paper-bg-surface}, 0 0 0 4px {colors.intent-action-accent}"

  # ── 输入：草稿卡里的字段「看起来是文本、其实可改」
  field-inline:
    backgroundColor: "transparent"
    textColor: "{colors.fg-primary}"
    typography: "{typography.money-sm}"
    rounded: "{rounded.sm}"
    border: "0"
  field-inline-hover:
    backgroundColor: "transparent"
    border: "0 0 1px {colors.intent-pending-accent} solid"
  field-inline-focus:
    backgroundColor: "{colors.paper-bg-field}"
    border: "1px solid {colors.intent-action-accent}"
    rounded: "{rounded.sm}"
  field-inline-invalid:
    backgroundColor: "{colors.intent-alarm-surface}"
    textColor: "{colors.intent-alarm-text}"
    border: "1px solid {colors.intent-alarm-accent}"
  field-boxed:
    backgroundColor: "{colors.paper-bg-field}"
    textColor: "{colors.fg-primary}"
    typography: "{typography.body-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.2} {spacing.3}"
    height: 32px
    border: "1px solid {colors.paper-border-default}"
  field-boxed-focus:
    border: "1px solid {colors.intent-action-accent}"
    shadow: "0 0 0 2px {colors.paper-bg-surface}, 0 0 0 4px {colors.intent-action-accent}"
  field-boxed-disabled:
    backgroundColor: "{colors.paper-bg-disabled}"
    textColor: "{colors.fg-faint}"

  # ── 徽章：kind 说「是什么」，status 说「怎么样」，两件事分开表达
  badge-kind:
    backgroundColor: "transparent"
    textColor: "{colors.fg-muted}"
    typography: "{typography.num-meta}"
    rounded: "{rounded.sm}"
    padding: "2px {spacing.1}"
    border: "1px solid {colors.rail-border-default}"
  badge-status-pending:
    backgroundColor: "{colors.intent-pending-surface}"
    textColor: "{colors.intent-pending-text}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "2px {spacing.2}"
  badge-status-settled:
    backgroundColor: "{colors.intent-settled-surface}"
    textColor: "{colors.intent-settled-text}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "2px {spacing.2}"
  badge-status-alarm:
    backgroundColor: "{colors.intent-alarm-surface}"
    textColor: "{colors.intent-alarm-text}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "2px {spacing.2}"
  badge-status-action:
    backgroundColor: "{colors.intent-action-surface}"
    textColor: "{colors.intent-action-text}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "2px {spacing.2}"

  # ── 提示条：左侧 2px 竖线 + 淡底 + 深字，不用图标承担语义
  banner-alarm:
    backgroundColor: "{colors.intent-alarm-surface}"
    textColor: "{colors.intent-alarm-text}"
    typography: "{typography.body-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.3} {spacing.4}"
    border: "0 0 0 2px {colors.intent-alarm-accent} solid"
  banner-pending:
    backgroundColor: "{colors.intent-pending-surface}"
    textColor: "{colors.intent-pending-text}"
    typography: "{typography.body-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.3} {spacing.4}"
    border: "0 0 0 2px {colors.intent-pending-accent} solid"
  banner-progress:
    backgroundColor: "{colors.intent-action-surface}"
    textColor: "{colors.intent-action-text}"
    typography: "{typography.body-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.3} {spacing.4}"
    border: "0 0 0 2px {colors.intent-action-accent} solid"
  banner-settled:
    backgroundColor: "{colors.intent-settled-surface}"
    textColor: "{colors.intent-settled-text}"
    typography: "{typography.body-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.3} {spacing.4}"
    border: "0 0 0 2px {colors.intent-settled-accent} solid"

  # ── 草稿卡：全产品密度最高的组件，四个状态必须一眼分清
  draft-card:
    backgroundColor: "{colors.draft-surface}"
    textColor: "{colors.fg-primary}"
    rounded: "{rounded.md}"
    padding: "{spacing.3} {spacing.4}"
    border: "1px solid {colors.paper-border-subtle}"
  draft-card-picked:
    backgroundColor: "{colors.paper-bg-surface-raised}"
    rounded: "{rounded.md}"
    border: "0 0 0 3px {colors.draft-marker-picked} solid"
    shadow: "0 1px 2px rgba(33,18,8,.07)"
  draft-card-gap:
    backgroundColor: "{colors.draft-surface}"
    rounded: "{rounded.md}"
    border: "1px solid {colors.draft-border-gap}"
  draft-card-machine-written:
    backgroundColor: "{colors.draft-surface}"
    textColor: "{colors.fg-muted}"
    rounded: "{rounded.md}"
    border: "1px solid {colors.draft-marker-written}"
  list-row-source:
    backgroundColor: "transparent"
    textColor: "{colors.fg-primary}"
    typography: "{typography.body-md}"
    padding: "{spacing.3} {spacing.4}"
    height: 44px
    border: "0 0 1px {colors.rail-border-subtle} solid"
  list-row-source-selected:
    backgroundColor: "{colors.rail-bg-surface-raised}"
    border: "0 0 0 3px {colors.intent-action-fill} solid"
  list-row-source-failed:
    textColor: "{colors.intent-alarm-text}"
    backgroundColor: "transparent"

  # ── 证据：原件那张纸是不可改写内容的唯一容器
  evidence-sheet:
    backgroundColor: "{colors.evidence-sheet}"
    rounded: "{rounded.none}"
    shadow: "0 18px 46px rgba(33,18,8,.18), inset 0 0 60px rgba(155,114,37,.10)"
  evidence-desk:
    backgroundColor: "{colors.evidence-desk}"
    rounded: "{rounded.none}"
  evidence-quote:
    backgroundColor: "{colors.evidence-quote-bg}"
    textColor: "{colors.fg-secondary}"
    typography: "{typography.serif-quote}"
    padding: "{spacing.3} {spacing.4}"
    border: "0 0 0 2px {colors.evidence-rule} solid"

  # ── M1 新面的落点
  progress-parse:
    backgroundColor: "{colors.intent-action-border}"
    rounded: "{rounded.full}"
    height: 4px
  progress-parse-fill:
    backgroundColor: "{colors.intent-action-accent}"
    rounded: "{rounded.full}"
    height: 4px
  agent-log-line:
    backgroundColor: "transparent"
    textColor: "{colors.fg-muted}"
    typography: "{typography.num-meta}"
  agent-log-line-current:
    textColor: "{colors.intent-action-text}"
  toggle-on:
    backgroundColor: "{colors.intent-settled-fill}"
    rounded: "{rounded.full}"
  toggle-off:
    backgroundColor: "{colors.paper-bg-disabled}"
    rounded: "{rounded.full}"
  notice:
    backgroundColor: "{colors.ink-900}"
    textColor: "{colors.ink-100}"
    typography: "{typography.body-md}"
    rounded: "{rounded.md}"
    padding: "{spacing.3} {spacing.4}"
    shadow: "0 24px 60px rgba(33,18,8,.26)"
  card:
    backgroundColor: "{colors.paper-bg-surface-raised}"
    rounded: "{rounded.md}"
    padding: "{spacing.4}"
    border: "1px solid {colors.paper-border-subtle}"
    shadow: "0 1px 2px rgba(33,18,8,.06), 0 4px 12px rgba(33,18,8,.08)"
  overlay:
    backgroundColor: "{colors.paper-bg-surface-raised}"
    rounded: "{rounded.lg}"
    padding: "{spacing.5}"
    shadow: "0 24px 60px rgba(33,18,8,.26)"
  dropzone:
    backgroundColor: "{colors.paper-bg-surface}"
    textColor: "{colors.fg-muted}"
    rounded: "{rounded.md}"
    padding: "{spacing.8}"
    border: "1px dashed rgba(33,18,8,.30)"
---

# Daybook 桌面端 Token Design System v0.3

> 出处：Claude Design 项目「Daybook桌面端设计」的 `Daybook Token System v3.dc.html`（v0.3 · 全亮），2026-08-14 落到本仓库。
> **状态：待评审，不是定稿。** 仓库根 [`CLAUDE.md`](./CLAUDE.md)「当前状态」写明 M1 开工前才确定 token design system；当前 `src/` 的界面是 M0 功能基线，**尚未按本文实现**。
> 图形的事实源在 [`assets/brand/README.md`](./assets/brand/README.md)，不在这里——本文只取它的颜色。

## Overview

M0 的界面已经把概念说对了：一张纸摊在工作台上，纸上是不可改写的原件，右侧是等待人确认的草稿。v0.3 保留那个概念，但把整个应用改成**全亮**：底色就是 Cloud Dancer 那一档米白 `{colors.paper-200}`，棕墨只作为文字、图标与线。桌面工具整天开着，大面积深色会压人——层次改由四级浅面加一根发丝线承担，不靠明暗反转。**全亮界面里唯一的深色面是通知条**（`{colors.ink-900}`）。

颜色全部取自应用图标：棕墨 `#4E2F1C`、麻色 `#C4B79C`、那抹蓝 `#A9C6E8`、底板米白 `#F1EFEA`。四个语义色同亮度（L .58）同彩度（C .105）站在一个色环上——靛青 action、松绿 settled、麻黄 caution、朱砂 alarm。**彩度压到 .105 是刻意的**：它们要和图标里那些哑光颜色坐在一起，不是和 SaaS 的霓虹坐在一起。结构上参考 Notion 的取向：中性面占绝大多数、淡色底承载分类、一个签名色只留给主行动、方角而非胶囊。

**三层单向依赖**，这是整套体系的骨架：

```
primitive（1.x 的色阶，不带含义）
   → semantic（带含义，按区域重映射）
      → component（只在组件内部使用）
```

**组件样式里出现任何 primitive（`ink-700`、`#f0ede8`）都算违规**，可以直接写成一条 lint 规则。

### 两种写法指的是同一个 token

本文有两套 token 写法，**不是笔误**：

- **`{colors.ink-700}`** —— 引用本文 frontmatter 里的 key（kebab-case，机器可解析，`npx @google/design.md lint` 查的就是它）
- **`ink.700` / `color.bg.surface`** —— 设计稿原文的写法（点号分层）。语义层与领域层的表格保留这一套，因为**那些 key 本身就是设计稿的产物**，改写成 kebab-case 会让人对不回设计稿

映射规则只有一条：**点号换成连字符**。`color.bg.surface` 这类按区域重映射的 key 在 frontmatter 里被展平成 `paper-bg-surface` / `rail-bg-surface` 两条——因为 YAML 装不下「同一个 key 按区域取不同值」，那正是 CSS 变量该干的事（见下面的 L2 代码块）。

### 与应用图标的关系

图标是取色来源，但 **token 不是图标 hex 的复制品**——它们被 oklch 规整过：

| 图标里的颜色 | 对应 token | 差异 |
|---|---|---|
| `#F1EFEA` 底板 | `{colors.paper-200}` `#f0ede8` | ΔRGB 2 |
| `#4E2F1C` 棕墨 | `{colors.ink-800}` `#463021` | ΔRGB 8 |
| `#C4B79C` 麻 | `{colors.paper-500}` `#c1b6a2` | ΔRGB 6 |
| `#A9C6E8` 那抹蓝 | `{colors.brand-200}` `#b4d6f3` | ΔRGB 16 |

**不要为了「统一」去改图标的 hex。** 图标有自己的事实源（[`assets/brand/README.md`](./assets/brand/README.md)），它的色板服务于 512px 的图形，token 服务于界面文字的对比度，两套值各自成立。

> ⚠️ **设计稿里的图标是 13a 之前的版本。** 2026-08-13 图标改用了方案 13a「平移居中」（最外层加一个 `translate`，把纸堆挪到画布正中）。**13a 只挪位置，四个取色源一个都没变**，所以上表与整套取色论证不受影响；但设计稿里那张 `assets/icon.svg` 的构图已经过时，以仓库的 [`assets/brand/icon.svg`](./assets/brand/icon.svg) 为准。

## Colors

> 源真值是 `oklch()`，表里的 sRGB 是它在 sRGB 下的取值。**16 个中性档已逐个验算**：oklch → sRGB 与下表 hex 完全一致（唯一例外 `paper-50` 差 1/255）。

### 中性色阶：两条阶承担 99% 的界面

`ink`（暖调棕墨，hue 55）**只负责文字、图标与线**；`paper`（棉纸，hue 82）**负责所有纸面**。

| Token | oklch（源真值） | sRGB | 典型用途 · 对比度 |
|---|---|---|---|
| `{colors.ink-950}` | `oklch(.20 .030 55)` | `#211208` | 所有主文字 15.6:1 · 通知条底 |
| `{colors.ink-900}` | `oklch(.26 .035 55)` | `#311f13` | 通知条 / 图表最深档 —— 全亮界面里唯一的深色面 |
| `{colors.ink-800}` | `oklch(.33 .040 55)` | `#463021` | 图标那支棕本色 · 通知条上的抬升块 |
| `{colors.ink-700}` | `oklch(.42 .030 55)` | `#5a493e` | 纸面次级文字 7.3:1 |
| `{colors.ink-600}` | `oklch(.52 .022 55)` | `#73665d` | 纸面弱化文字 4.8:1 —— **正文可读性下限** |
| `{colors.ink-500}` | `oklch(.62 .018 55)` | `#8f847c` | 占位符 / 分隔 / 禁用；纸面 3.1:1 *仅装饰* |
| `{colors.ink-400}` | `oklch(.72 .015 55)` | `#aca29c` | 禁用文字 / 草稿斜纹 / 图表次环 |
| `{colors.ink-300}` | `oklch(.82 .013 55)` | `#cbc2bc` | 通知条上的次级文字 |
| `{colors.ink-200}` | `oklch(.89 .010 55)` | `#e0d9d5` | 冷调分隔线 / 禁用填充 |
| `{colors.ink-100}` | `oklch(.945 .008 55)` | `#f1ebe8` | 通知条上的主文字 |
| `{colors.paper-50}` | `oklch(.99 .004 82)` | `#fdfcf9` | 灯箱上的原件纸 / 卡片抬升面 |
| `{colors.paper-100}` | `oklch(.97 .006 82)` | `#f7f5f1` | 内容面板底（审核栏、账目、设置） |
| `{colors.paper-200}` | `oklch(.948 .008 82)` | `#f0ede8` | **应用底色 · Cloud Dancer**：顶栏与来源夹 |
| `{colors.paper-300}` | `oklch(.92 .011 82)` | `#e8e4dd` | 工作台桌面（灯箱背后那层）· 下沉块 |
| `{colors.paper-400}` | `oklch(.885 .016 82)` | `#ded8ce` | 禁用填充 / 打印纸边 / 边框 |
| `{colors.paper-500}` | `oklch(.78 .030 82)` | `#c1b6a2` | 纸面上的边框 / 禁用按钮填充 |

### 四个语义色：共享 L 与 C，只换 hue

每个色只需要 5 档：`100` 淡底、`200` 边框、`500` 图标与色条、`600` 白字可压的实底、`700` 纸面上的文字。

**规则：500 不写小字，文字用 600/700。** 这条规则在语义层被固化成了三个不同的名字——`accent`(500) / `fill`(600) / `fill-hover`(700)，见下面「意图语义」。

| 色 | hue | 100 | 200 | 500 | 600 | 700 | 产品里的唯一含义 |
|---|---|---|---|---|---|---|---|
| 朱砂 critical | 30 | `#fdece8` | `#f3c4bc` | `#b06154` | `#8a4338` | `#663028` | 差额报警、解析失败、丢弃草稿、删除。**全产品最稀缺的颜色，一屏最多一处实底。** |
| 麻黄 caution | 80 | `#f7efe2` | `#e4cea9` | `#9b7225` | `#785300` | `#583d00` | 需要人处理但不危险：待确认计数、需补全三元组、无法校验、背书提示。 |
| 松绿 positive | 150 | `#e7f4e9` | `#b8dcbe` | `#488c58` | `#296a3b` | `#1d4e2b` | 已落定的事实：已入账、账已对上、开关已授权。**草稿永远不许用这个颜色。** |
| 靛青 brand | 245 | `#e6f2fd` | `#b4d6f3` | `#3e80b4` | `#1f5f8f` | `#15466a` | 你发起的一切：主按钮、焦点环、链接、解析进度、agent 日志当前行。**一屏只有一个实底。** |

四个 `500` 档已验算：都精确等于 `oklch(.58 .105 <hue>)`，「同亮度同彩度只换 hue」这条成立。

### L2 · 区域语义（region）

Daybook 同屏有两种**用途不同的区域**：`rail`（顶栏、来源夹——工具的外壳）与 `paper`（灯箱、审核栏——内容本身）。**两者都是浅色，差别只有一档面色与一根线**，文字色两边完全相同。

**语义层的 key 不写具体颜色、只按区域重映射，这就是留给未来主题的接口**：将来要加暗色主题＝新增一组区域块，组件一行不用改。本版只实现 `theme=cloud` 这一套。

```css
/* L1 · primitive：只在这里出现字面值 */
:root {
  --ink-950: oklch(.20 .030 55);
  --paper-200: oklch(.948 .008 82);
  --brand-600: oklch(.47 .100 245);
  /* … */
}
/* L2 · semantic：按区域重映射 */
[data-theme="cloud"] [data-region="paper"] {
  --color-bg-surface: var(--paper-100);
  --color-fg-primary: var(--ink-950);
  --color-fg-secondary: var(--ink-700);
}
[data-theme="cloud"] [data-region="rail"] {
  --color-bg-surface: var(--paper-200);
  --color-fg-primary: var(--ink-950);
  --color-fg-secondary: var(--ink-700);
}
/* L3 · component：只引用 L2 */
.button--primary {
  background: var(--color-intent-action-fill);
  color: var(--color-intent-action-on-fill);
  border-radius: var(--radius-sm);
}
```

| semantic key | `data-region="paper"` | 对比度 | `data-region="rail"` | 对比度 |
|---|---|---|---|---|
| `color.bg.canvas` | `{colors.paper-300}` | — | `{colors.paper-300}` | — |
| `color.bg.surface` | `{colors.paper-100}` | — | `{colors.paper-200}` | — |
| `color.bg.surface.raised` | `{colors.paper-50}` | — | `{colors.paper-50}` | — |
| `color.bg.surface.sunken` | `{colors.paper-300}` | — | `{colors.paper-300}` | — |
| `color.bg.field` | `{colors.paper-50}` | — | `{colors.paper-50}` | — |
| `color.bg.selected` | `{colors.brand-100}` | — | `{colors.paper-50}` | — |
| `color.bg.hover` | ink α.04 | — | ink α.035 | — |
| `color.bg.pressed` | ink α.08 | — | ink α.07 | — |
| `color.bg.disabled` | `{colors.paper-500}` | — | `{colors.paper-400}` | — |
| `color.fg.primary` | `{colors.ink-950}` | **16.7** | `{colors.ink-950}` | **15.6** |
| `color.fg.secondary` | `{colors.ink-700}` | 7.9 | `{colors.ink-700}` | 7.3 |
| `color.fg.muted` | `{colors.ink-600}` | 5.1 | `{colors.ink-600}` | 4.8 |
| `color.fg.faint` | `{colors.ink-500}` | *3.4* | `{colors.ink-500}` | *3.1* |
| `color.border.subtle` | ink α.10 | — | ink α.10 | — |
| `color.border.default` | ink α.18 | — | ink α.16 | — |
| `color.border.strong` | ink α.45 | — | — | — |
| `color.fg.numeral` | — | — | `{colors.caution-700}` | 8.6 |

对比度那两列 = 该色作为文字压在本区域 `color.bg.surface` 上的值，**已逐条验算，与设计稿声称的数字全部相符（误差 < 0.05）**。

- **`fg.faint` 未过 AA**（3.4 / 3.1），仅限占位符、分隔符、禁用文本等非信息性用途。
- **最深的面是 `bg.canvas`（`{colors.paper-300}`）**，`fg.muted` 压在它上面只有 **4.38** —— 在桌面上放正文时降一档用 `fg.secondary`。
- **rail 与 paper 的文字色完全一致**：区域只换面色，不换字色，这样同一个组件搬到任何区域都不用改 token。
- `fg.numeral`（来源计数那个数字）用 `{colors.caution-700}`，它是图标麻色压深后的那一档。

### L2 · 意图语义（intent，不随区域变化）

**「差额报警」在深栏和纸上必须是同一个红**，否则闸门的严重性就被主题稀释了。

**机器状态没有意图色**：AI 写入、草稿、未确认一律用中性 `ink` + 45° 斜纹表达，所以意图色只有这四个。

| intent | `.surface` | `.border` | `.accent` | `.fill` | `.fill-hover` | `.text` |
|---|---|---|---|---|---|---|
| | 100 淡底 | 200 边框 | **500 图标/进度/色条/描边** | **600 能压白字的实底** | 700 | 700 纸面上的文字 |
| `intent.settled` 松绿 | `#e7f4e9` | `#b8dcbe` | `#488c58` | `#296a3b` | `#1d4e2b` | `#1d4e2b` |
| `intent.pending` 麻黄 | `#f7efe2` | `#e4cea9` | `#9b7225` | `#785300` | `#583d00` | `#583d00` |
| `intent.alarm` 朱砂 | `#fdece8` | `#f3c4bc` | `#b06154` | `#8a4338` | `#663028` | `#663028` |
| `intent.action` 靛青 | `#e6f2fd` | `#b4d6f3` | `#3e80b4` | `#1f5f8f` | `#15466a` | `#15466a` |

**`.accent`（L .58）只做图标、进度、色条、描边，永不承载文字。** `.fill` 是能压白字的实底，其上的文字统一用 `intent.*.on-fill` = `{colors.paper-50}`（白纸色，非纯白）：

| intent | `paper.50` on `.accent` | on `.fill` | on `.fill-hover` |
|---|---|---|---|
| action | 4.13 ❌ | **6.62** ✓ | 9.11 ✓ |
| settled | 3.96 ❌ | **6.36** ✓ | 8.86 ✓ |
| pending | 4.24 ❌ | **6.74** ✓ | 9.26 ✓ |
| alarm | 4.37 ❌ | **6.96** ✓ | 9.56 ✓ |

**`.accent` 那一列全部不过 AA，这正是它和 `.fill` 必须是两个名字的原因**——只有一个 `.fill` 时，「按钮底」和「进度条」会挑同一个值，而其中一个必然是错的。`.text` 档专供压在 `.surface` 或主纸面上的文字（实测 8.5–9.1:1）。

> 这套六列是**本仓库相对设计稿的一处偏离**，见下面「相对设计稿的偏离」。

### L2 · 领域语义（evidence / draft / ledger）

「AI 永不直接写入账本」（[ADR-0002](./docs/adr/0002-ai-never-writes-directly.md)、[`CLAUDE.md`](./CLAUDE.md) 约束 3）这条生死线**要在 token 层就有名字**，否则实现时会退化成随手挑色。这一组是产品概念的直接投影，任何新面（时间轴、账目、图表）都必须复用它们表达同一件事。

| semantic | 解析为 | 含义 · 使用约束 |
|---|---|---|
| `color.evidence.sheet` | `paper.50` + `shadow.sheet` | 原件那张纸。**不可改写内容的唯一容器。** |
| `color.evidence.desk` | `paper.300` + 6px 点阵 | 灯箱桌面。点阵是「这是工作台不是内容」的信号。 |
| `color.evidence.rule` | `caution.200` | 纸边的测量尺 / 引文左侧双线。 |
| `color.evidence.quote.bg` | `paper.300` | 「原件位置」引文块底。`font.display` 15/1.9。 |
| `color.draft.surface` | `paper.50` α.85 | 草稿卡片底：比确认后的账目*更轻*，视觉上是「未落定」。 |
| `color.draft.marker.picked` | `brand.600` + `ring.marker` | 人已勾选。**人的动作用靛青，不用松绿**——它还没入账。 |
| `color.draft.marker.written` | `ink.400` + 45° 斜纹 | AI 刚写入 / 本轮新增。**机器写的东西不配拥有颜色**——只给中性灰与纹理，这是「AI 永不直接写入账本」在 token 层的形状。 |
| `color.draft.border.gap` | `critical.500` α.5 | 该条无法折算 / 总额不参与校验。 |
| `color.ledger.fact` | `ink.950` on `paper.100` | 已入账事实：满对比、实底、不带任何 α。**与草稿的差别必须一眼看出。** |
| `color.ledger.expense / income` | `ink.950` / `positive.700` | **支出不用红**——红只留给报警。收入才着色。 |

### 图表色序（为 M1 统计图预留）

分类色序不是新调色板，而是把意图色的色环补满：第一环 `oklch(.58 .105 h)`，第二环 `oklch(.72 .075 h)`，hue 每 45° 取一档。同环内亮度一致，**灰度打印仍能靠 hue 区分**。

| Token | oklch | sRGB | |
|---|---|---|---|
| `{colors.chart-1}` | `oklch(.58 .105 150)` | `#488c58` | ＝ `positive.500` |
| `{colors.chart-2}` | `oklch(.58 .105 245)` | `#3e80b4` | ＝ `brand.500` |
| `{colors.chart-3}` | `oklch(.58 .105 80)` | `#9b7225` | ＝ `caution.500` |
| `{colors.chart-4}` | `oklch(.58 .105 30)` | `#b06154` | ＝ `critical.500` |
| `{colors.chart-5}` | `oklch(.72 .075 195)` | `#68b4b3` | |
| `{colors.chart-6}` | `oklch(.72 .075 300)` | `#ac9acd` | |
| `{colors.chart-7}` | `oklch(.72 .075 120)` | `#9eac76` | 设计稿标注 115°，实际取值 120°，见「待澄清 ①」 |
| `{colors.chart-8}` | `oklch(.72 .075 15)` | `#cf9295` | |

**序号 1–4 与意图色重合，因此图表里不得用序号 1–4 表达「正常分类」之外的含义。**

## Typography

三种声音，各有明确职责，**互不越界**。宋体用**思源宋体**（Noto Serif SC）而非 Songti SC——它有真实的 500/600/700 字重，小字号下不发虚；Latin 与数字交给 **IBM Plex** 家族，等宽那支自带表格数字，金额对齐不需要额外设置。

| 族 | 栈 | 职责 |
|---|---|---|
| `font.display` | `"Noto Serif SC", "Songti SC", "STSong", serif` | 只用于标题、空状态、商户名、原件引文。**不用于 UI 控件。** |
| `font.sans` | `"IBM Plex Sans", "PingFang SC", "Noto Sans SC", system-ui` | 界面语言：标签、说明、按钮、菜单、正文。 |
| `font.mono` | `"IBM Plex Mono", "SFMono-Regular", Consolas, monospace` | 凡是「机器写的、要逐位核对的」：金额、汇率、日期、币种、schema、错误码、区段标签。 |

| Token | size / lh / ls | 族 | 示例 |
|---|---|---|---|
| `{typography.display-lg}` | 46 / 1.24 / -.005em | display | 把零散的钱和事，整理清楚。 |
| `{typography.display-md}` | 36 / 1.26 / 0 | display | 你的个人事务助理 |
| `{typography.title-lg}` | 27 / 1.3 / 0 | display | 4 条待确认 |
| `{typography.title-md}` | 21 / 1.35 / 0 | display | 一段口述 |
| `{typography.title-sm}` | 17 / 1.4 / 0 | display | Coles 日用采买 |
| `{typography.body-lg}` | 15 / 1.6 / 0 | sans | 拖入截图，或说一段话。不用逐条填表。 |
| `{typography.body-md}` | 13 / 1.6 / 0 | sans | **界面默认字号。** |
| `{typography.body-sm}` | 12 / 1.55 / 0 | sans | 辅助说明与列表次级信息。 |
| `{typography.label}` | 11 / 1.4 / .02em | sans | 控件标签 · **最小可读字号，低于 11px 一律不允许** |
| `{typography.eyebrow}` | 11 / 1 / .16em | mono | 来源原件 · 不可改写 |
| `{typography.serif-quote}` | 15 / 1.9 / 0 | display | “上周三在 Coles 买菜 62.4，周四午饭三明治 11.5” |
| `{typography.money-lg}` | 28 / 1.1 / 0 · 600 | mono | 1,482.60 |
| `{typography.money-md}` | 20 / 1.2 / 0 · 600 | mono | 62.40 AUD |
| `{typography.money-sm}` | 15 / 1.2 / 0 · 600 | mono | 300.00 CNY |
| `{typography.num-meta}` | 11 / 1.4 / .04em | mono | 1 CNY = 0.210000 AUD · 2026-08-08 · schema 6 |

> 「族」这一列是**按 §1.2 的职责描述推导的**，设计稿的字号表本身没有逐 token 标注字体族。见「待澄清 ③」。

## Layout

### 间距：4px 栅格

桌面端密度高，`{spacing.2}`–`{spacing.4}` 承担大部分内间距；`{spacing.1}` 只用于图标与文字之间。

`{spacing.px}` 1 · `{spacing.half}` 2 · `{spacing.1}` 4 · `{spacing.2}` 8 · `{spacing.3}` 12 · `{spacing.4}` 16 · `{spacing.5}` 20 · `{spacing.6}` 24 · `{spacing.8}` 32 · `{spacing.10}` 40 · `{spacing.12}` 48 · `{spacing.16}` 64

### 三栏工作台

| Token | 值 | 说明 |
|---|---|---|
| `{spacing.rail-width}` | 252px | 来源夹固定宽 |
| `{spacing.tray-min}` – `{spacing.tray-max}` | 390–460px | 审核栏区间 |
| `{spacing.sheet-max}` | 780px | 原件纸最大宽 |

### 层级 z-index

| Token | 值 | 用于 |
|---|---|---|
| `z.base` | 0 | 三栏工作台 |
| `z.sticky` | 10 | 栏内吸顶头、确认栏 |
| `z.overlay` | 100 | 弹层、菜单 |
| `z.toast` | 400 | 通知条 |

## Elevation & Depth

**阴影一律带暖色**（rgba 取自 `ink` 而非纯黑），落在暖纸上不发灰。

| Token | 值 | 用于 |
|---|---|---|
| `shadow.raise` | `0 1px 2px rgba(33,18,8,.07)` | 抬起一档：已勾选的草稿卡 |
| `shadow.card` | `0 1px 2px rgba(33,18,8,.06), 0 4px 12px rgba(33,18,8,.08)` | 卡片 |
| `shadow.sheet` | `0 18px 46px rgba(33,18,8,.18), inset 0 0 60px rgba(155,114,37,.10)` | 灯箱上的原件纸 |
| `shadow.overlay` | `0 24px 60px rgba(33,18,8,.26)` | 弹层、通知条 |
| `ring.focus` | `0 0 0 2px <纸色>, 0 0 0 4px {colors.intent-action-accent}` | 焦点环：先垫一圈纸色再上靛青 |
| `ring.marker` | `inset 3px 0 {colors.brand-600}` | 选中 / 来源标记 |

## Shapes

沿用 Notion 那套「方角而非胶囊」的取向，**但比它收一档**：控件 6、卡片 10、弹层 14。胶囊只留给徽章与状态点，**大面板一律直角——纸不圆**。

| Token | 值 | 用于 |
|---|---|---|
| `{rounded.none}` | 0 | 面板 / 纸 |
| `{rounded.sm}` | 6px | 控件 |
| `{rounded.md}` | 10px | 卡片 |
| `{rounded.lg}` | 14px | 弹层 |
| `{rounded.full}` | 999px | 徽章 / 状态点 |

描边（primitive 档，语义档见「区域语义」表）：

| Token | 值 |
|---|---|
| `border.hairline` | 1px `rgba(33,18,8,.14)` |
| `border.default` | 1px `rgba(33,18,8,.24)` |
| `border.strong` | 2px `rgba(33,18,8,.55)` |
| `border.marker` | 3px `{colors.brand-600}` —— 「这一条被选中/被标记」的专用语汇，来自 M0 的左侧色条 |
| `border.dashed` | 1px dashed `rgba(33,18,8,.30)` —— 拖放区 |

## Motion

这是一个「人在核对数字」的工具，**动效只用来说明状态变化，不做表演**。

| Token | 值 | 用于 |
|---|---|---|
| `motion.instant` | 80ms | hover / 选中态 |
| `motion.fast` | 140ms | 按钮、卡片、输入 |
| `motion.base` | 200ms | 面板切换、通知进出 |
| `motion.slow` | 320ms | 灯箱换片、抽屉 |
| `motion.pulse` | 1600ms | 解析中：brand 呼吸，**唯一的循环动画** |
| `ease.standard` | `cubic-bezier(.2,0,0,1)` | |
| `ease.exit` | `cubic-bezier(.4,0,1,1)` | |

**全部 token 在 `prefers-reduced-motion` 下降为 0ms**，仅进度条保留静态进度。这一条与 [`assets/brand/README.md`](./assets/brand/README.md) 里 `loading.svg` 的无障碍处理同源：动效停下来，信息不能跟着消失。

## Components

> 尺寸单位 px。**全部状态显式列出**——「hover 时大概深一点」不是规格。

### 按钮

三档高度：`lg` 40（批量入账这类终局动作）、`md` 32（默认）、`sm` 26（卡内动作）。

**一屏只允许一个 primary。** 「丢弃」这类破坏性动作是 `button-alarm-ghost`，**不给实底**——实底红会把注意力从差额报警上偷走。

| 组件 | 默认 | 说明 |
|---|---|---|
| `button-primary` | `action.fill` 底 / `on-fill` 字 | 确认所选入账 |
| `button-secondary` | `bg.surface.raised` 底 / `border.default` 描边 | 设定本位币 |
| `button-ghost` | 透明 / `action.text` 字，无边框 | 单条确认 |
| `button-alarm-ghost` | 透明 → hover `alarm.surface` / `alarm.text` 字 | 丢弃 |

四档状态：`default` / `hover`（`.fill-hover`）/ `active`（`bg.pressed`）/ `focus-visible`（`ring.focus`）/ `disabled`（`bg.disabled` + `fg.faint`）。**按钮搬到 `rail` 区域用同一套 token**，区域只换面色。

### 输入

草稿卡里的字段是**「看起来是文本、其实可改」**：静止时无框，hover 出现 `caution` 下划线，focus 才成为完整输入框。**这条规则来自 M0，保留**——它让审核像在纸上圈改，而不是在填表。

**金额一律 `font.mono`。**

| 组件 | 用于 | rest → hover → focus → invalid |
|---|---|---|
| `field-inline` | 草稿卡内可改字段 | 无框 → `caution` 下划线 → 完整框 + 焦点环 → `alarm` 底 + 错误说明 |
| `field-boxed` | 设置 / 补全 / 表单 | 常规框 → — → 焦点环 → `alarm` 描边 + 错误说明 |

`invalid` 必须带一句人话（「金额只能是数字与小数点」「需要 3 位 ISO 4217 代码」），不是只把框染红。

### 徽章与状态点

状态徽章只有两种形态，**「是什么」与「怎么样」分开表达**：

- **`badge-kind`**（方框、等宽、描边）——来源的类型（`IMG` / `SAY` / `PDF`），尺寸固定，**永不变色**
- **`badge-status`**（胶囊、带点）——生命周期，取意图色：等待解析 / 正在还原 / N 条待确认 / 已归档 / 解析失败
- **`badge-reconcile`**——账已对上（settled）/ 差额报警（alarm）/ 无法校验（pending）/ 请你背书（pending）
- **`badge-agent`**——考古员已就绪 / 正在检查 Claude Code / Claude Code 尚未登录

### 提示条

统一结构：**左侧 2px `intent.*.accent` 竖线 + `intent.*.surface` 底 + `intent.*.text` 文字**。

**不用图标承担语义**（图标只在 `alarm` 用一个圈叹号）——因为一屏可能同时出现三条，**颜色比图标更快分辨**。

| 组件 | intent | 典型内容 |
|---|---|---|
| `banner-alarm` | alarm | 合计对不上 · 来源声明 1,482.60 · 草稿合计 1,420.20 |
| `banner-pending` | pending | 这份来源有未读区域 |
| `banner-progress` | action | 正在还原这份来源里的交易 |
| `banner-settled` | settled | 已确认 4 条交易 |

> `banner-alarm` 那条与 [`docs/prd/03-review.md` §3.4](./docs/prd/03-review.md) 是同一件事：口述来源对账 `failed` 时批量确认**照常放行**，但差额与「确认前请对着原文过一遍」必须与按钮同屏。**放行而不告知，等于两道闸门都没有。**

### 草稿卡与列表行

草稿卡是**全产品密度最高的组件**。四个状态必须一眼分清：

| 状态 | 表现 | 为什么 |
|---|---|---|
| `rest` | `draft.surface`（`paper.50` α.85） | 比确认后的账目更轻——「未落定」 |
| `picked` | `ring.marker`（靛青）+ `shadow.raise` | 人已勾选。**人的动作用靛青，不用松绿** |
| `gap` | `draft.border.gap`（`critical.500` α.5） | 需补全本位币与汇率，挡住确认 |
| `machine-written` | `ink.400` + 45° 斜纹 | AI 刚写入。**中性灰 + 纹理，不占语义色** |

来源夹的列表行（`rail` 区域）：

- **行高最小 44** —— 桌面端点击目标下限
- 内边距 `{spacing.3}` 上下 / `{spacing.4}` 左右
- `selected`：抬一档到 `bg.surface.raised` + `ring.marker`。**被选中的来源就是「摊到灯箱上的那张纸」，靠抬升而不是靠反色**
- `failed`：次级文字换 `critical.700`，**行底不变色**——失败是一条信息，不是一次报警
- 分隔线 `border.subtle`，**最后一行不画**

### M1 新面的落点

时间轴、账目、图表、设置、空/错/载入五类状态**都必须在现有 token 里找到落点，不允许为新面引入新色**。

- **`timeline.row`**（钱与事同轴）：实心点＝已入账事实；空心松绿＝收入；灰点 + 72% 不透明＝仍是草稿。**事实与草稿在同一轴上但绝不长得一样。**
- **`chart.bar` / `chart.axis`**：只用图表色序。**未确认部分永远是中性斜纹——图表里也不能让草稿冒充事实。** 轴线 `border.subtle`，刻度文字 `fg.muted` 11/mono。
- **`settings.row` / `toggle`**：开关的「开」只用 `settled` 绿——它表示**你已授权**，与入账同一语义家族。
- **`progress.parse`**：解析是**你发起的动作，用 `action` 不用 `pending`**。进度条 4px、`{rounded.full}`、底 `action.border`、**进度本身 `action.accent`**（它是色条，不是实底）；日志等宽 10–11px、`fg.muted`，当前行升到 `action.text`。
- **`empty.hero`**：`display.md` + eyebrow + `body.lg`，只在灯箱区出现。栏内空态不用插画。
- **`error.panel`**：**错误必须带错误码与去处** —— 「这份来源没能解析完 / 模型返回的结构不完整，账本没有任何改动 / `agent.malformed_result` / 重新解析 · 查看本机日志」。错误码集见 [`docs/prd/00-foundation.md` §3.7](./docs/prd/00-foundation.md)。
- **`notice`**：**全亮界面里唯一的深色面**（`{colors.ink-900}`），只用于短暂通知。

## Do's and Don'ts

这 8 条是设计稿的「落地检查清单」，**逐条都能写成 lint 规则或 code review 判据**：

1. 组件样式里出现任何 primitive（`ink-700`、`#f0ede8`）＝**违规**，改成 semantic。
2. **一屏一个 `button-primary`**；破坏性动作永远 ghost。
3. 正文与信息性文字对比度 **≥ 4.5**；`fg.faint` / `fg.numeral` 只许装饰或 ≥18px 600。
4. **字号不得低于 11px**；金额、汇率、日期、错误码必须 `font.mono`。
5. **松绿只标记已落定的事实**；靛青是行动与进行中。**草稿不许用松绿。**
6. **草稿与事实在任何视图**（列表、时间轴、图表）**都必须有可见差别。**
7. 全亮：深色面只允许出现在通知条。控件 6 / 卡片 10 / 弹层 14，大面板直角。
8. 所有动效在 `prefers-reduced-motion` 下降为 0ms，仅 `progress` 保留静态进度。

第 5、6 两条不是审美偏好，是 [ADR-0002](./docs/adr/0002-ai-never-writes-directly.md) 与 [`CLAUDE.md`](./CLAUDE.md) 约束 3 在像素层的形状：**AI 产出的东西必须看起来就没落定。**

## Iteration Guide

1. 一次只动一个组件。
2. 直接引用 token 名与组件名，不要在讨论里报 hex。
3. 改完跑 `npx @google/design.md lint design.md`。
4. 新变体作为独立的 `components:` 条目新增，不要给旧条目加分支。
5. 正文默认 `{typography.body-md}`（13px）。
6. **签名色靛青只给主行动**（`{colors.intent-action-fill}` 实底 / `{colors.intent-action-accent}` 色条），链接与主按钮共享它，但一屏只有一个实底。
7. 按钮用 `{rounded.sm}`，卡片 `{rounded.md}`，弹层 `{rounded.lg}`，`{rounded.full}` 只给徽章与状态点。

## 相对设计稿的偏离

**只有一处，但它改了语义层的形状，所以单列一节。**

### 意图色从五列变成六列（2026-08-14 定）

设计稿的 intent 表是 `.surface / .border / .fill / .fill-hover / .text` 五列。**本文是六列**——在 `.fill` 前面插了一个 `.accent`。

**为什么**：设计稿同一段里有两句话互相矛盾——

> 「`.fill` 上的文字统一用 `intent.*.on-fill = paper.50`：action 6.6:1 / settled 6.4:1 / pending 6.7:1 / alarm 7.0:1，**全部过 AA**。」
> 「`.fill`（L .58）只做图标、进度、色条，**不承载小字**。」

实测把两句话分开了：**那四个声称的对比度全部是 600 档的值，而设计稿 `.fill` 那一列画的是 500 档**（`settled` 行除外——它本来就画的 600，与另外三行不齐）。500 档实测 3.96–4.37，**四个全部不过 AA**。

也就是说，设计稿的 `.fill` 同时被要求做两件互相冲突的事：**当按钮底（要能压白字）** 和 **当进度条/图标（要那个更亮的调子）**。一个 token 满足不了两个约束，所以拆成两个：

| | 设计稿 | 本文 | 干什么 |
|---|---|---|---|
| — | — | `.accent` = 500 | 图标、进度、色条、描边、焦点环。**永不承载文字** |
| `.fill` | 500 | `.fill` = 600 | 能压白字的实底：主按钮、开关的「开」 |
| `.fill-hover` | 600 | `.fill-hover` = 700 | 实底的 hover |

`.surface` / `.border` / `.text` 三列不动。**`settled` 那一行的取值一个都没变**——它本来就是对的，这也是这次拆分方向正确的旁证。

**受影响的组件引用**（都在本文 `components:` 里）：`button-primary` 与 `button-primary-lg` 取 `.fill`；banner 的 2px 竖线、`progress-parse-fill`、`field-*-focus` 的焦点环与描边取 `.accent`；`list-row-source-selected` 的 3px 标记线取 `.fill`（＝ `brand.600`，与 `ring.marker`、`color.draft.marker.picked` 同值）。

> 这一处需要**回流到设计稿**：Claude Design 项目里的 `Daybook Token System v3.dc.html` 仍是五列。

## 待澄清

**这三条是把设计稿转成本文时发现的，不是实现问题——动手实现前值得先定。**

### ① `chart.7` 是 115° 还是 120°？

设计稿的标签写 `115°`，但那个色块的实际取值是 `oklch(.72 .075 120)`。**本文按实际取值记 120°**（`#9eac76`）；若应为 115°，正确值是 `#a1ac74`。第二环若要严格按「每 45° 一档」，从 195° 起应是 195 / 240 / 285 / 330，而设计稿给的是 195 / 300 / 120(115) / 15——**实际是绕整个色环取的补位，不是等距 45°**，措辞可以再收一次。

### ② 禁用态的文字压在禁用填充上只有 1.82:1

设计稿把 `color.bg.disabled` 定为 `paper.500`（paper 区）/ `paper.400`（rail 区），又把 `ink.500` / `ink.400` 标为「禁用文字」。两者一撞，实测：

| 禁用文字 | 压在 `paper.500` | 压在 `paper.400` |
|---|---|---|
| `ink.700` `#5a493e` | 4.26 | 6.03 |
| `ink.600` `#73665d` | 2.77 | 3.91 |
| `ink.500` `#8f847c` | **1.82** | 2.57 |
| `ink.400` `#aca29c` | 1.25 | 1.76 |

WCAG 对禁用控件不作对比度要求，**所以这不算违规**——但 1.82:1 已经接近读不出来，而禁用按钮上的字正是用户需要看清才知道「为什么点不动」的那句。**paper 区连 `ink.700` 都只到 4.26**，说明问题在底色而不在字色：`paper.500` 作为禁用填充太深了。建议要么禁用填充降到 `paper.400`（`ink.700` 达 6.03），要么禁用态改成「不变底、只降字色 + 去掉描边」。

### ③ 字号表没有逐 token 标注字体族

设计稿 §1.2 用散文规定了三个族的职责，但字号表本身没有「族」这一列。本文那一列是**按职责描述推导的**：`display.*`/`title.*`/`serif.quote` → display，`body.*`/`label` → sans，`eyebrow`/`money.*`/`num.meta` → mono。**`eyebrow` 归 mono 是按设计稿自身的渲染反推的**（它渲染成等宽 + `.16em` 字距），值得确认。

## Lint 状态

```bash
npx @google/design.md@0.4.0 lint design.md
```

**0 error。** 剩下的 warning 分三类，**每一类都是刻意留的，不要靠删内容去清零**：

| 规则 | 数量级 | 为什么留着 |
|---|---|---|
| `broken-ref`（`border` / `shadow` 不是它认识的 sub-token） | ~30 | v0.4.0 只认 `backgroundColor / textColor / typography / rounded / padding / size / height / width`。**但发丝线与 `ring.marker` 正是这套体系的承重墙**——「被选中的来源」全靠 3px 靛青内嵌线表达。删掉它们能清零 warning，也能让规格失去一半信息。参考用的 `notion-DESIGN.md` 同样在用这两个字段。 |
| `contrast-ratio`（9 条） | 9 | **7 条是误报**：linter 把 `backgroundColor: transparent` 当成纯黑算。那 7 个组件实际压在 `paper.100` / `paper.200` / `draft.surface` 上，实算 4.75–17.73:1，全部过 AA。它们必须保持透明——**同一个组件搬到 rail 或 paper 都不改 token，靠的就是继承区域面色**。剩下 2 条（禁用态）是真的，见「待澄清 ②」。 |
| `orphaned-tokens`（~79） | ~79 | 未被任何 `components:` 条目引用的 primitive 与 semantic。**色阶本身就是规格**——`ink.100`–`ink.950` 十档要成阶才能让人挑，不是每一档都得有组件用。图表色序整组也在这里，它是给 M1 预留的。 |

7 条误报的实算值（跑一次就能复核）：

| 组件 | 实际底色 | 实算 |
|---|---|---|
| `field-inline` | `draft.surface` | 17.73 |
| `list-row-source` | `paper.200` | 15.57 |
| `button-alarm-ghost` | `paper.100` | 9.56 |
| `button-ghost` | `paper.100` | 9.11 |
| `list-row-source-failed` | `paper.200` | 8.91 |
| `agent-log-line` | `paper.100` | 5.09 |
| `badge-kind` | `paper.200` | 4.75 |

## 相关

- [`CLAUDE.md`](./CLAUDE.md) —— 17 条实施约束；约束 3（AI 永不直接写入）与本文的领域语义是同一件事
- [`assets/brand/README.md`](./assets/brand/README.md) —— 应用图标与加载动画，**图形的事实源**，本文只取它的颜色
- [`.claude/rules/frontend.md`](./.claude/rules/frontend.md) —— 前端实现细则；审核界面的键盘流与「并排的必须是原件」在那里
- [`docs/prd/03-review.md`](./docs/prd/03-review.md) —— 审核界面规格，本文的草稿卡 / 提示条 / 徽章都是它的视觉投影
- [ADR-0002](./docs/adr/0002-ai-never-writes-directly.md) —— 为什么草稿不许长得像事实
