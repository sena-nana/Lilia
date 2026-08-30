# Lilia Native 样式标准

LiliaCode 使用 NanaUI 控件、主题令牌与 WGPU 渲染能力。通用控件、窗口装饰、布局原语和主题能力应先进入 NanaUI；`apps/desktop` 只维护产品页面编排与业务状态。

## 原则

- 保持工程工具气质：克制、清晰、可扫描。
- 视觉层级为主内容、当前状态、过程信息/辅助操作。
- 深浅主题均从共享令牌派生，不在业务页面硬编码颜色。
- 可见控件必须连接真实行为；不可用能力明确禁用，不提供占位交互。
- 新页面优先组合现有 NanaUI 控件；重复模式上移为公共控件，不在业务模块复制样式。
- Agent Debug 稳定目标属于语义层，不依赖像素坐标、显示文本或临时布局结构。

## 布局标准写法

- 容器默认竖排（`FlexDirection::Column` 是缺省值）；水平排列必须显式使用 `Stack` 预设（`row` / `fill_row` / `bar`）。已有 shell 装配可继续 `HostStack`；新的独立容器用 `Stack`，不手写 `LayoutStyle` 布局字段。
- 主内容区用 `fill_column`（Fill 高 + grow 1）；`column` 只用于高度随内容的纵向结构。主区不伸展、底部输入区不贴底，基本都是这里用错。
- 需要收缩的子项显式写 `shrink(1.0)`：未写 `flex_shrink` 按 0 处理，不是 CSS 的 1；`align_items` 缺省也是 `Start` 而非 `stretch`。
- 对话框、菜单、抽屉、气泡用 NanaUI 浮层控件（`Dialog`、`ActionMenu`、`Popover`、`Drawer`）并锚定触发控件槽位，不用绝对定位自摆。

## 边框标准写法

- 边框颜色（`NodeStyle.border` 语义角色）与宽度（`LayoutStyle.border_width`）分属两个字段，缺一边不绘制且无警告；必须经 `outline(role, width)` 一次写全。
- 卡片容器标准三件套：`surface(SemanticColorRole::Surface)` + `outline(SemanticColorRole::Border, 1.0)` + `radius(...)`，参考 `composer_card()`。
- `Card` 控件用 `kind(CardKind::Outlined)` 拿描边；用户 `.style(...)` 显式给出的背景、边框、圆角优先于 kind 默认值。
- 颜色一律 `SemanticColorRole`；裸 `[f32;4]` 与主题脱节，不允许进业务模块。

## 评审清单

- 控件、间距、排版、状态色是否复用 NanaUI 公共能力。
- 行列排布是否用 `Stack`/`HostStack` 预设，fill 与 shrink 语义是否正确，无误竖排。
- 边框是否成对（颜色+宽度）书写，深浅主题下均可见。
- 主操作、状态和辅助信息层级是否清楚。
- hover、active、disabled、focus 与窗口焦点行为是否完整。
- resize、DPI、长文本、空状态、加载、错误与深浅主题是否可读。
- 页面是否只承担产品特有布局和业务语义。
