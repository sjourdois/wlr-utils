# 简体中文翻译（界面文本）— wlr-chooser。

tab-all = 全部
tab-windows = 窗口
tab-outputs = 屏幕
filter-hint = 筛选…
screen-label = 屏幕 { $name }
loading = …
preview-unavailable = 预览不可用
show-system = 系统窗口
error = wlr-chooser: { $error }
capture-no-window = 此合成器无法捕获单个窗口。窗口捕获需要 ext-image-copy-capture-v1 及 foreign-toplevel 源（wlroots >= 0.20 / Sway >= 1.12）。运行 `wlr-peek doctor` 查看你的合成器支持哪些功能。
focus-unsupported = 此合成器无法聚焦你选择的窗口。聚焦窗口需要 wlr-foreign-toplevel-management-v1，在 COSMIC 上则需要 cosmic-toplevel-management-v1。运行 `wlr-peek doctor` 查看你的合成器支持哪些功能。
mru-unmatched = 此合成器的焦点历史与列出的任何窗口都不对应，因此 --window-order mru 退回为按名称排序。
filter-no-match = 没有打开的窗口匹配 { $filter }。没有可显示的内容。
pid-unsupported = 此合成器不报告窗口所属的进程，因此无法应用 --pid。这需要 sway、Hyprland 或 niri 的 IPC。运行 `wlr-peek doctor` 查看是否检测到焦点后端。
hints-need-keyboard = --hints 需要没有筛选输入框的布局：请使用 --layout strip 或 --layout grid。
grid-needs-card = --grid 用于确定卡片大小，因此需要 --layout card（默认）。总览和横排会自行排布。
