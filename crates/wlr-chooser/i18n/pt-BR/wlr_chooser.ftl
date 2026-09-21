# Tradução em português do Brasil (textos da interface) do wlr-chooser.

tab-all = Tudo
tab-windows = Janelas
tab-outputs = Telas
filter-hint = Filtrar…
screen-label = Tela { $name }
loading = …
preview-unavailable = Pré-visualização indisponível
show-system = Janelas do sistema
error = wlr-chooser: { $error }
capture-no-window = Este compositor não pode capturar janelas individuais. A captura de janelas requer ext-image-copy-capture-v1 com a fonte foreign-toplevel (wlroots >= 0.20 / Sway >= 1.12). Execute `wlr-peek doctor` para ver o que seu compositor suporta.
focus-unsupported = Este compositor não pode focar a janela escolhida. Focar uma janela requer wlr-foreign-toplevel-management-v1, ou cosmic-toplevel-management-v1 no COSMIC. Execute `wlr-peek doctor` para ver o que seu compositor suporta.
mru-unmatched = O histórico de foco deste compositor não nomeia nenhuma das janelas listadas, então --window-order mru volta à ordenação por nome.
filter-no-match = Nenhuma janela aberta corresponde a { $filter }. Nada a mostrar.
pid-unsupported = Este compositor não informa o processo ao qual uma janela pertence, então --pid não pode ser aplicado. Isso requer o IPC do sway, Hyprland ou niri. Execute `wlr-peek doctor` para ver se um backend de foco foi detectado.
