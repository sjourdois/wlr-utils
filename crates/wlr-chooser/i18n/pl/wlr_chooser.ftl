# Polskie tłumaczenie (teksty interfejsu) wlr-chooser.

tab-all = Wszystko
tab-windows = Okna
tab-outputs = Ekrany
filter-hint = Filtruj…
screen-label = Ekran { $name }
loading = …
preview-unavailable = Podgląd niedostępny
show-system = Okna systemowe
error = wlr-chooser: { $error }
capture-no-window = Ten kompozytor nie może przechwytywać pojedynczych okien. Przechwytywanie okien wymaga ext-image-copy-capture-v1 ze źródłem foreign-toplevel (wlroots >= 0.20 / Sway >= 1.12). Uruchom `wlr-peek doctor`, aby zobaczyć, co obsługuje twój kompozytor.
focus-unsupported = Ten kompozytor nie może aktywować wybranego okna. Aktywacja okna wymaga wlr-foreign-toplevel-management-v1 lub cosmic-toplevel-management-v1 w systemie COSMIC. Uruchom `wlr-peek doctor`, aby zobaczyć, co obsługuje twój kompozytor.
mru-unmatched = Historia fokusu tego kompozytora nie wskazuje żadnego z wypisanych okien, więc --window-order mru wraca do sortowania według nazwy.
filter-no-match = Żadne otwarte okno nie pasuje do { $filter }. Nie ma nic do pokazania.
pid-unsupported = Ten kompozytor nie podaje procesu, do którego należy okno, więc nie można zastosować --pid. Wymaga to IPC sway, Hyprland lub niri. Uruchom `wlr-peek doctor`, aby sprawdzić, czy wykryto backend fokusu.
scratchpad-unsupported = Ten kompozytor nie ma scratchpada, między którego oknami można by się przełączać, więc nie można zastosować --scratchpad. Wymaga to IPC sway ($SWAYSOCK). Uruchom `wlr-peek doctor`, aby sprawdzić, czy wykryto backend fokusu.
scratchpad-empty = Scratchpad nie zawiera żadnego okna. Nie ma nic do pokazania.
scratchpad-not-put-aside = Kompozytor nie odłożył aktywnego okna z powrotem do scratchpada.
hints-need-keyboard = --hints wymaga układu bez pola filtra: użyj --layout strip lub --layout grid.
grid-needs-card = --grid ustala rozmiar karty, więc wymaga --layout card (domyślnie). Ekspozycja i pasek układają się same.
daemon-already-running = Demon wlr-overlayd już działa.
daemon-none = Żaden demon wlr-overlayd nie działa.
daemon-gone = Demon wlr-overlayd zakończył działanie bez odpowiedzi.
daemon-not-running = Żaden demon wlr-overlayd nie działa, więc ta nakładka płaci pełny koszt uruchomienia (około 90 ms). Uruchom `wlr-overlayd` wraz z sesją, aby pojawiała się natychmiast, albo podaj --no-daemon, jeśli tego właśnie chcesz — wtedy ten komunikat zniknie. Zobacz `wlr-overlayd --help` lub README wlr-chooser.
daemon-busy = Demon wlr-overlayd pokazuje już nakładkę, więc ta jest pokazywana tutaj i płaci pełny koszt uruchomienia.
daemon-bypassed = To uruchomienie nie przechodzi przez demona — --no-gpu zmienia zachowanie całego procesu — więc płaci pełny koszt uruchomienia.
daemon-cannot-serve = Demon nie może obsłużyć tego uruchomienia: --no-gpu i --doctor zmieniają zachowanie całego procesu. Uruchom je z --no-daemon.
hold-no-modifier = Tryb przytrzymania jest włączony: przełącznik przełącza, gdy tylko żaden klawisz Alt ani Super nie jest przytrzymany — od razu i bez pokazywania czegokolwiek, jeśli żaden nie jest przytrzymany przy otwarciu. Podaj --no-hold, aby nakładka pozostała otwarta.
