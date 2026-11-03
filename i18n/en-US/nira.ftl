# Nira's English strings. Every other locale falls back to these.
# Ids are kebab-case and grouped by the part of the UI they belong to.

## Keyboard actions (shortcut editor, menus). `action-<name>` where <name>
## is the action's name in keys.toml.

action-next-page = Next page
action-previous-page = Previous page
action-forward-ten-pages = Forward ten pages
action-back-ten-pages = Back ten pages
action-first-page = First page
action-last-page = Last page
action-go-to-page = Go to page…
action-scroll-up = Scroll up
action-scroll-down = Scroll down
action-scroll-left = Scroll left
action-scroll-right = Scroll right
action-smart-scroll-up = Smart scroll up
action-smart-scroll-down = Smart scroll down
action-zoom-in = Zoom in
action-zoom-out = Zoom out
action-zoom-reset = Reset zoom
action-fit-best = Best fit
action-fit-width = Fit width
action-fit-height = Fit height
action-fit-size = Fit to size
action-fit-manual = Manual zoom
action-rotate-cw = Rotate clockwise
action-rotate-ccw = Rotate counter-clockwise
action-rotate-180 = Rotate 180°
action-flip-horizontal = Flip horizontally
action-flip-vertical = Flip vertically
action-double-page = Double page
action-manga-mode = Manga mode (right to left)
action-fullscreen = Fullscreen
action-exit-fullscreen = Leave fullscreen
action-thumbnails = Show thumbnails
action-toolbar = Show toolbar
action-hide-all = Hide all controls
action-slideshow = Slideshow
action-invert-smart-scroll = Invert smart scroll
action-page-info = Page info
action-minimize = Minimize
action-add-bookmark = Add bookmark
action-edit-bookmarks = Edit bookmarks…
action-lens = Magnifying lens

## Fit modes (View panel dropdown)

fit-best = Best fit
fit-width = Fit width
fit-height = Fit height
fit-size = Fit to size
fit-manual = Manual

## Status and page labels

# $current: first page shown, $total: page count
page-of = { $current } / { $total }
# A two-page spread, pages in screen order.
pages-of = { $first }–{ $second } / { $total }
pages-count = { $count ->
    [one] { $count } page
   *[other] { $count } pages
}

## Window

open-tooltip = Open a book (Ctrl+O)
open-dialog-title = Open a book

## Errors

open-failed = Cannot open the book
open-error = { $file }: { $reason }
open-error-list = Cannot list the pages of { $file }: { $reason }
open-error-empty = { $file } has no pages.

## Toolbar

toolbar-open = Open
toolbar-previous-book = Previous file in folder
toolbar-next-book = Next file in folder
# $pages: "37" or "37–38" for a spread
toolbar-progress = Page { $pages } of { $total } · { $percent }% read
toolbar-fit = Fit mode
toolbar-bookmark = Bookmark this page
toolbar-lens = Magnifying lens ({ $key })
toolbar-view-settings = View settings
toolbar-focus = Focus mode (full screen)
layout-single = Single
layout-spread = Spread
direction-ltr = Comic L→R
direction-rtl = Manga R→L

## Status bar

hint-turn-page = Turn page
hint-focus-mode = Focus mode

## View panel

panel-title = View
panel-close = Close view settings
panel-image = Image
panel-auto-enhance = Auto-enhance
panel-auto-enhance-hint = Levels, sharpen, de-yellow scans
panel-brightness = Brightness
panel-contrast = Contrast
panel-background = Background
background-black = Black
background-grey = Grey
background-paper = Paper
background-auto = Auto
panel-reading = Reading
panel-cover-alone = First page alone in spreads
panel-auto-hide = Fade the page bar while reading
panel-page-bar = Show the page bar
panel-focus-progress = Progress line in focus mode
panel-slideshow = Slideshow every
panel-reset = Reset to defaults

## Sidebar

sidebar-pages = Pages
sidebar-bookmarks = Bookmarks
sidebar-no-bookmarks = No bookmarks in this file
sidebar-bookmark-page = Page { $page }
# $size is already formatted ("38.2 MB")
sidebar-footer = { $pages ->
    [one] { $pages } page
   *[other] { $pages } pages
} · { $size }

## Focus mode

focus-hint = Move to the top edge for controls
focus-exit = Exit focus

## Start screen

welcome-title = Open something to read
welcome-sub = No library to manage — just open a file or a folder of images.
welcome-drop = Drop a comic file or folder here
welcome-or = or
welcome-open-file = Open file…
welcome-open-folder = Open folder…
welcome-image-folder = JPG / PNG / WEBP folder
welcome-continue = Continue reading
welcome-clear-history = Clear history
welcome-history-empty = Books you open will appear here.
welcome-remembered = Your place in each book is remembered.
welcome-finished = Finished
welcome-progress = p. { $page } / { $total }
when-today = Today
when-yesterday = Yesterday
open-folder-dialog-title = Open a folder of images

## Lens

# $zoom: "2.5"
lens-label = Lens { $zoom }× · scroll to change

## Preferences

panel-all-settings = All settings…
panel-about = About
prefs-title = Preferences
prefs-tab-reading = Reading
prefs-tab-scrolling = Scrolling
prefs-tab-files = Files
prefs-tab-keys = Keys
prefs-fit = Fitting
prefs-default-fit = Fit mode
prefs-fit-size-axis = “Fit to size” limits
prefs-axis-width = Width
prefs-axis-height = Height
prefs-axis-both = Width and height
prefs-fit-size-px = “Fit to size” size (px)
prefs-scale-up = Enlarge small pages
prefs-scale-up-hint = Fitting may scale pages above 100 %.
prefs-spreads = Spreads
prefs-start-double = Open books in Spread layout
prefs-start-manga = Open books right to left (manga)
prefs-wide-alone = Show wide pages alone
prefs-wide-alone-hint = A page wider than tall (a scanned spread) is not paired.
prefs-double-step = Turn two pages at a time in spreads
prefs-window = Window
prefs-start-fullscreen = Start in focus mode
prefs-exif = Rotate photos by their EXIF orientation
prefs-exif-hint = For pages taken with a camera or phone.
prefs-appearance = Appearance
prefs-theme = Theme
prefs-theme-system = Follow the system (Nira Dark or Light)
prefs-page-numbers = Page numbers under thumbnails
prefs-smart-scroll = Space bar
prefs-smart-scroll-on = Smart scroll
prefs-smart-scroll-hint = Space reads through the spread in reading order, then turns the page.
prefs-invert-hint = Read across before down.
prefs-smart-step = Step (% of the window)
prefs-keys-wheel = Arrow keys and wheel
prefs-key-step = Arrow key step (px)
prefs-wheel-step = Wheel step (px)
prefs-flip-wheel = Turn pages at the page edges
prefs-flip-wheel-hint = Scrolling past the top or bottom of a page turns it.
prefs-edge-presses = Presses at the edge before turning
prefs-books = Books
prefs-open-next = Continue into the next book
prefs-open-next-hint = Past the last page, open the next file in the folder.
prefs-open-next-folder = Continue into the next folder
prefs-slideshow-next = Slideshow continues into the next book
prefs-reopen-last = Reopen the last book at startup
prefs-escape-quits = Escape quits when not in focus mode
prefs-memory = Memory
prefs-cached-pages = Decoded pages kept in memory

## Keys

keys-hint = Press + and then the new key. A key used by another action moves to this one.
keys-press = Press a key… (Esc cancels)
keys-add = Add a key
keys-clear = Remove all keys
keys-reset = Restore the default keys
keys-reset-all = Restore all default keys
go-to-page-go = Go

## About

about-comments = A comic book and manga viewer. Reads ZIP, RAR, 7Z, TAR, LHA and PDF archives, and folders of images.
about-website = Project website

## Colour themes (Preferences › Appearance). `theme-<id>` for each theme
## in crates/nira/src/themes/.

theme-dark = Nira Dark
theme-light = Nira Light
theme-solarized-dark = Solarized Dark
theme-solarized-light = Solarized Light
theme-vscode-dark = VS Code Dark
theme-vscode-light = VS Code Light
theme-nord = Nord
theme-dracula = Dracula
theme-gruvbox-dark = Gruvbox Dark
theme-gruvbox-light = Gruvbox Light
theme-catppuccin-mocha = Catppuccin Mocha
theme-catppuccin-latte = Catppuccin Latte
