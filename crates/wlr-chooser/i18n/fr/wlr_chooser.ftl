# Chaînes d'interface de wlr-chooser (la ligne de commande, via clap, est en anglais).

tab-all = Tout
tab-windows = Fenêtres
tab-outputs = Écrans
filter-hint = Filtrer…
screen-label = Écran { $name }
loading = …
preview-unavailable = Aperçu indisponible
show-system = Fenêtres système
error = wlr-chooser : { $error }
capture-no-window = Ce compositeur ne peut pas capturer de fenêtres individuelles. La capture de fenêtre nécessite ext-image-copy-capture-v1 avec la source foreign-toplevel (wlroots >= 0.20 / Sway >= 1.12). Lancez `wlr-peek doctor` pour voir ce que prend en charge votre compositeur.
focus-unsupported = Ce compositeur ne peut pas donner le focus à la fenêtre choisie. Donner le focus nécessite wlr-foreign-toplevel-management-v1, ou cosmic-toplevel-management-v1 sur COSMIC. Lancez `wlr-peek doctor` pour voir ce que prend en charge votre compositeur.
mru-unmatched = L'historique de focus de ce compositeur ne désigne aucune des fenêtres listées : --window-order mru retombe sur l'ordre alphabétique.
filter-no-match = Aucune fenêtre ouverte ne correspond à { $filter }. Rien à afficher.
pid-unsupported = Ce compositeur n’indique pas le processus auquel appartient une fenêtre, --pid ne peut donc pas être appliqué. Cela nécessite l’IPC de sway, Hyprland ou niri. Lancez `wlr-peek doctor` pour voir si un backend de focus a été détecté.
hints-need-keyboard = --hints nécessite une présentation sans champ de filtre : utilisez --layout strip ou --layout grid.
grid-needs-card = --grid dimensionne la carte : il nécessite --layout card (le défaut). L’exposé et la bande se disposent d’eux-mêmes.
