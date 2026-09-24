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
scratchpad-unsupported = Ce compositeur n’a pas de scratchpad entre les fenêtres duquel basculer, --scratchpad ne peut donc pas être appliqué. Cela nécessite l’IPC de sway ($SWAYSOCK). Lancez `wlr-peek doctor` pour voir si un backend de focus a été détecté.
scratchpad-empty = Le scratchpad ne contient aucune fenêtre. Rien à afficher.
scratchpad-not-put-aside = Le compositeur n’a pas remis la fenêtre active dans le scratchpad.
hints-need-keyboard = --hints nécessite une présentation sans champ de filtre : utilisez --layout strip ou --layout grid.
grid-needs-card = --grid dimensionne la carte : il nécessite --layout card (le défaut). L’exposé et la bande se disposent d’eux-mêmes.
daemon-already-running = Un démon wlr-overlayd tourne déjà.
daemon-none = Aucun démon wlr-overlayd ne tourne.
daemon-gone = Le démon wlr-overlayd s'est arrêté sans répondre.
daemon-not-running = Aucun démon wlr-overlayd ne tourne : cet overlay paie donc tout le démarrage (environ 90 ms). Lancez `wlr-overlayd` avec votre session pour qu'il soit instantané, ou passez --no-daemon si c'est ce que vous voulez et cet avis disparaîtra. Voir `wlr-overlayd --help` ou le README de wlr-chooser.
daemon-busy = Le démon wlr-overlayd affiche déjà un overlay ; celui-ci est donc affiché ici et paie tout le démarrage.
daemon-bypassed = Cette exécution ne passe pas par le démon — --no-gpu change le comportement de tout le processus — elle paie donc tout le démarrage.
daemon-cannot-serve = Un démon ne peut pas prendre en charge cette exécution : --no-gpu et --doctor changent le comportement de tout le processus. Lancez-la avec --no-daemon.
hold-no-modifier = Le mode maintien est actif : le sélecteur bascule dès qu'aucune touche Alt ou Super n'est tenue — aussitôt, sans rien afficher, si aucune ne l'est à son ouverture. Passez --no-hold pour garder l'overlay ouvert.
