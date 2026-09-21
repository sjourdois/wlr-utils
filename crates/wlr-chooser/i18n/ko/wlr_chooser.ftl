# 한국어 번역 (UI 문자열) — wlr-chooser.

tab-all = 전체
tab-windows = 창
tab-outputs = 화면
filter-hint = 필터…
screen-label = 화면 { $name }
loading = …
preview-unavailable = 미리 보기를 사용할 수 없음
show-system = 시스템 창
error = wlr-chooser: { $error }
capture-no-window = 이 컴포지터는 개별 창을 캡처할 수 없습니다. 창 캡처에는 ext-image-copy-capture-v1과 foreign-toplevel 소스(wlroots >= 0.20 / Sway >= 1.12)가 필요합니다. `wlr-peek doctor`를 실행하여 컴포지터가 지원하는 기능을 확인하세요.
focus-unsupported = 이 컴포지터는 선택한 창에 포커스를 줄 수 없습니다. 창 포커스에는 wlr-foreign-toplevel-management-v1이, COSMIC에서는 cosmic-toplevel-management-v1이 필요합니다. `wlr-peek doctor`를 실행하여 컴포지터가 지원하는 기능을 확인하세요.
mru-unmatched = 이 컴포지터의 포커스 기록이 나열된 어떤 창과도 일치하지 않으므로 --window-order mru는 이름순 정렬로 대체됩니다.
filter-no-match = { $filter }와(과) 일치하는 열린 창이 없습니다. 표시할 것이 없습니다.
pid-unsupported = 이 컴포지터는 창이 속한 프로세스를 알려주지 않으므로 --pid를 적용할 수 없습니다. sway, Hyprland 또는 niri의 IPC가 필요합니다. `wlr-peek doctor`를 실행하여 포커스 백엔드가 감지되었는지 확인하세요.
hints-need-keyboard = --hints는 필터 입력란이 없는 레이아웃이 필요합니다. --layout strip 또는 --layout grid를 사용하세요.
grid-needs-card = --grid는 카드 크기를 정하므로 --layout card(기본값)가 필요합니다. 엑스포제와 스트립은 스스로 배치됩니다.
daemon-already-running = wlr-overlayd 데몬이 이미 실행 중입니다.
daemon-none = 실행 중인 wlr-overlayd 데몬이 없습니다.
daemon-gone = wlr-overlayd 데몬이 응답하지 않고 종료되었습니다.
daemon-not-running = 실행 중인 wlr-overlayd 데몬이 없어 이 오버레이는 시작 비용을 전부 치릅니다(약 90 ms). 세션과 함께 `wlr-overlayd`를 실행하면 즉시 표시됩니다. 의도한 것이라면 --no-daemon을 쓰세요. 이 알림도 사라집니다. `wlr-overlayd --help` 또는 wlr-chooser README를 참고하세요.
daemon-busy = wlr-overlayd 데몬이 이미 다른 오버레이를 표시하고 있어 이 오버레이는 여기서 표시되며 시작 비용을 전부 치릅니다.
daemon-bypassed = 이 실행은 데몬을 거치지 않습니다. --no-gpu가 프로세스 전체의 동작을 바꾸기 때문이며, 시작 비용을 전부 치릅니다.
daemon-cannot-serve = 데몬은 이 실행을 맡을 수 없습니다. --no-gpu와 --doctor는 프로세스 전체의 동작을 바꿉니다. --no-daemon으로 실행하세요.
