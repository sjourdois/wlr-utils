# 日本語訳（UI 文字列）— wlr-chooser キャプチャツール群。

tab-all = すべて
tab-windows = ウィンドウ
tab-outputs = 画面
filter-hint = フィルター…
screen-label = 画面 { $name }
loading = …
preview-unavailable = プレビューを利用できません
show-system = システムウィンドウ
error = wlr-chooser: { $error }
capture-no-window = このコンポジタは個別のウィンドウをキャプチャできません。ウィンドウキャプチャには ext-image-copy-capture-v1 と foreign-toplevel ソース (wlroots >= 0.20 / Sway >= 1.12) が必要です。`wlr-peek doctor` を実行して、お使いのコンポジタが対応している機能を確認してください。
focus-unsupported = このコンポジタは選択したウィンドウをフォーカスできません。ウィンドウのフォーカスには wlr-foreign-toplevel-management-v1、COSMIC では cosmic-toplevel-management-v1 が必要です。`wlr-peek doctor` を実行して、お使いのコンポジタが対応している機能を確認してください。
mru-unmatched = このコンポジタのフォーカス履歴は、表示中のどのウィンドウとも対応しないため、--window-order mru は名前順にフォールバックします。
filter-no-match = { $filter } に一致する開いているウィンドウがありません。表示するものがありません。
pid-unsupported = このコンポジタはウィンドウの所属プロセスを報告しないため、--pid は適用できません。sway、Hyprland、niri のいずれかの IPC が必要です。`wlr-peek doctor` を実行して、フォーカスバックエンドが検出されたか確認してください。
hints-need-keyboard = --hints はフィルター入力欄のないレイアウトを必要とします。--layout strip または --layout grid を使ってください。
grid-needs-card = --grid はカードの大きさを決めるため、--layout card（既定）が必要です。エクスポゼとストリップは自動で配置されます。
daemon-already-running = wlr-switcher のデーモンはすでに実行中です。
daemon-none = wlr-switcher のデーモンは実行されていません。
daemon-gone = wlr-switcher のデーモンが応答せずに終了しました。ウィンドウは切り替わっていません。
daemon-cannot-serve = この実行はデーモンでは扱えません。--no-gpu と --doctor はプロセス全体の動作を変えるためです。--no-daemon を付けて実行してください。
daemon-not-running = wlr-switcher のデーモンが実行されていないため、このオーバーレイは起動処理をすべて負担します（約 90 ミリ秒）。セッションの自動起動から `wlr-switcher --daemon` を起動してください。
daemon-bypassed = この実行はデーモンを経由しません。--no-gpu はプロセス全体の動作を変えるためで、起動処理をすべて負担します。
