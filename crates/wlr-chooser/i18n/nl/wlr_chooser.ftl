# Nederlandse vertaling (interfaceteksten) van wlr-chooser.

tab-all = Alles
tab-windows = Vensters
tab-outputs = Schermen
filter-hint = Filteren…
screen-label = Scherm { $name }
loading = …
preview-unavailable = Voorbeeld niet beschikbaar
show-system = Systeemvensters
error = wlr-chooser: { $error }
capture-no-window = Deze compositor kan geen losse vensters vastleggen. Venstervastlegging vereist ext-image-copy-capture-v1 met de foreign-toplevel-bron (wlroots >= 0.20 / Sway >= 1.12). Voer `wlr-peek doctor` uit om te zien wat je compositor ondersteunt.
focus-unsupported = Deze compositor kan het gekozen venster niet focussen. Een venster focussen vereist wlr-foreign-toplevel-management-v1, of cosmic-toplevel-management-v1 op COSMIC. Voer `wlr-peek doctor` uit om te zien wat je compositor ondersteunt.
mru-unmatched = De focusgeschiedenis van deze compositor benoemt geen van de getoonde vensters; --window-order mru valt terug op sorteren op naam.
filter-no-match = Geen enkel geopend venster komt overeen met { $filter }. Niets te tonen.
pid-unsupported = Deze compositor meldt niet welk proces bij een venster hoort, dus --pid kan niet worden toegepast. Daarvoor is de IPC van sway, Hyprland of niri nodig. Voer `wlr-peek doctor` uit om te zien of er een focus-backend is gevonden.
scratchpad-unsupported = Deze compositor heeft geen scratchpad om tussen de vensters van te wisselen, dus --scratchpad kan niet worden toegepast. Daarvoor is de IPC van sway nodig ($SWAYSOCK). Voer `wlr-peek doctor` uit om te zien of er een focus-backend is gevonden.
scratchpad-empty = Het scratchpad bevat geen enkel venster. Niets te tonen.
scratchpad-not-put-aside = De compositor heeft het gefocuste venster niet teruggezet in het scratchpad.
hints-need-keyboard = --hints vereist een weergave zonder filterveld: gebruik --layout strip of --layout grid.
grid-needs-card = --grid bepaalt de grootte van de kaart en vereist dus --layout card (de standaard). De exposé en de strip delen zichzelf in.
daemon-already-running = Er draait al een wlr-overlayd-daemon.
daemon-none = Er draait geen wlr-overlayd-daemon.
daemon-gone = De wlr-overlayd-daemon is gestopt zonder te antwoorden.
daemon-not-running = Er draait geen wlr-overlayd-daemon, dus deze overlay betaalt de volledige opstart (ongeveer 90 ms). Start `wlr-overlayd` met je sessie om hem direct te laten verschijnen, of geef --no-daemon mee als je dat wilt — dan verdwijnt deze melding. Zie `wlr-overlayd --help` of de wlr-chooser-README.
daemon-busy = De wlr-overlayd-daemon toont al een overlay, dus deze wordt hier getoond en betaalt de volledige opstart.
daemon-bypassed = Deze uitvoering gaat niet via de daemon — --no-gpu verandert het gedrag van het hele proces — en betaalt dus de volledige opstart.
daemon-cannot-serve = Een daemon kan deze uitvoering niet overnemen: --no-gpu en --doctor veranderen het gedrag van het hele proces. Voer haar uit met --no-daemon.
hold-no-modifier = Vasthouden-om-te-wisselen staat aan: de wisselaar wisselt zodra er geen Alt- of Super-toets meer wordt vastgehouden — meteen en zonder iets te tonen als er bij het openen geen wordt vastgehouden. Geef --no-hold mee om de overlay open te houden.
