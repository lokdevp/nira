<p align="center">
  <img src="assets/nira.svg" alt="Nira logo" width="64">
</p>

<h1 align="center">Nira</h1>

<p align="center">A comic book and manga reader</p>

<p align="center"><a href="https://nira.lokify.me/">nira.lokify.me</a></p>

<p align="center">
  <img src="https://nira.lokify.me/screenshots/1.0/nira-dark.webp" alt="Nira showing a two-page spread of a comic, with the page thumbnails on the left and the View panel on the right">
</p>

Nira is a comic book and manga reader for Linux and Windows. It is written in Rust with GTK4. It is based on [MComix](https://sourceforge.net/projects/mcomix/), and through it on Comix. It is a rewrite with its own interface, settings and file formats.

## Features

- Single pages or double-page spreads, and a manga mode that reads right to left
- Fit to the window, width, height or size, or zoom by hand, with smart scrolling (Space) that moves through a page the way you read it
- Rotation and flipping
- A page sidebar with thumbnails, bookmarks, Go to page and a slideshow
- Focus mode, a magnifying lens and auto-enhance in the View panel
- A start screen with your reading history; books reopen at the page you left off
- Moving on to the previous or next book in the same folder
- Dark and light themes, or follow the desktop
- Every key can be rebound in Preferences

<table>
  <tr>
    <td><img src="https://nira.lokify.me/screenshots/1.0/nira-start-dark.webp" alt="The start screen: a drop area for opening a book, and a Continue reading list with covers and progress"></td>
    <td><img src="https://nira.lokify.me/screenshots/1.0/nira-focus-dark.webp" alt="Focus mode: a two-page spread filling the screen with only a thin progress line"></td>
    <td><img src="https://nira.lokify.me/screenshots/1.0/nira-light.webp" alt="Nira in the Light theme"></td>
  </tr>
  <tr>
    <td align="center">The start screen</td>
    <td align="center">Focus mode</td>
    <td align="center">The Light theme</td>
  </tr>
</table>

The comic pages in the screenshots are from <a href="https://www.peppercarrot.com/">Pepper&amp;Carrot</a> by David Revoy, under <a href="https://creativecommons.org/licenses/by/4.0/">CC BY 4.0</a>. More screenshots, including every colour theme, are on <a href="https://nira.lokify.me/">the website</a>.

## Supported formats

| | |
|---|---|
| Archives | ZIP/CBZ, RAR/CBR, 7z/CB7, TAR/CBT (also `.tar.gz`, `.tar.bz2`, `.tar.xz`), LHA/LZH, PDF, and plain folders of images |
| Images | JPEG, PNG, GIF, BMP, WebP, TIFF, ICO, PNM, TGA, QOI and JPEG 2000 |

ZIP, TAR and 7z work out of the box. Some formats need an external tool:

- RAR: `unrar` or `7z`
- LHA: `7z` or `lha`
- PDF: `mutool` (from mupdf-tools)
- 7z: uses `7z` when installed, and a built-in reader otherwise

Archives inside an archive aren't opened. Extract the outer archive first.

## Install

Download a package from the [Releases](https://github.com/lokdevp/nira/releases) page.

**Debian / Ubuntu**

```sh
sudo apt install ./nira_1.1.0-1_amd64.deb
```

The package recommends `unrar`, `p7zip-full` and `mupdf-tools` for RAR, 7z/LHA and PDF.

**Any Linux distribution (AppImage)**

```sh
chmod +x nira-1.1.0-x86_64.AppImage
./nira-1.1.0-x86_64.AppImage
```

**Tarball**

`nira-1.1.0-linux-x86_64.tar.gz` contains the `nira` binary, a desktop file and icons. GTK 4.12 or newer must be installed. See `INSTALL.txt` inside for how to run or install it.

**Windows**

- `nira-setup-1.1.0.exe` installs Nira for all users, adds it to the Start menu and registers it for comic book files. It becomes the default app for `.cbz`, `.cbr`, `.cbt` and `.cb7` unless another app already handles them, and is offered under *Open with* for PDFs, archives and images.
- `nira-1.1.0-windows-x86_64.zip` is a portable copy: unzip it anywhere and run `nira.exe`. It changes nothing in the registry, so it registers no file types.

The Windows builds are not code-signed. Windows SmartScreen warns about them the first time they run: choose *More info*, then *Run anyway*. To check a download first, compare its checksum with `SHA256SUMS` (see below).

ZIP, TAR and 7z books open out of the box. RAR, PDF and LHA need `7z`, `unrar` or `mutool` on your `PATH`.

**Verifying downloads**

Each release has a `SHA256SUMS` file. In the folder holding the downloads:

```sh
sha256sum -c --ignore-missing SHA256SUMS
```

On Windows, in PowerShell, print a file's checksum and compare it with its line in `SHA256SUMS`:

```powershell
Get-FileHash nira-setup-1.1.0.exe -Algorithm SHA256
```

To check that a file was built by Nira's release workflow, use the GitHub CLI:

```sh
gh attestation verify nira-1.1.0-x86_64.AppImage --repo lokdevp/nira
```

## Building from source

You need stable Rust and the GTK 4.12+ development files.

Debian / Ubuntu:

```sh
sudo apt install pkg-config libgtk-4-dev libgdk-pixbuf-2.0-dev \
  libpango1.0-dev libcairo2-dev libglib2.0-dev liblzma-dev libbz2-dev
```

Arch Linux:

```sh
sudo pacman -S --needed base-devel gtk4
```

Then:

```sh
cargo build --release      # the binary is target/release/nira
cargo test --workspace
```

The scripts for the `.deb` (`cargo deb -p nira`) and the AppImage are in [`packaging/`](packaging/).

## Usage

```sh
nira [OPTIONS] [PATH]
```

`PATH` is a comic archive, a PDF or a folder of images. Without one, Nira opens on the start screen. You can also drag a book onto the window.

| Option | |
|---|---|
| `-f`, `--fullscreen` | Start in fullscreen |
| `-m`, `--manga` | Start in manga mode (right to left) |
| `-d`, `--double-page` | Start in double-page mode |
| `--fit <MODE>` | Fit mode to start with: `best`, `width`, `height`, `size` or `manual` |
| `-p`, `--page <N>` | Open at page N (from 1) instead of where you left off |
| `-W`, `--log <LEVEL>` | Log level: `error`, `warn`, `info`, `debug` or `trace` |
| `--help` | Show help |

## Keyboard shortcuts

The main default keys:

| Key | Action |
|---|---|
| Page Down / Page Up, Backspace | Next / previous page |
| Shift+Page Down / Shift+Page Up | Forward / back ten pages |
| Home / End | First / last page |
| Space / Shift+Space | Smart scroll down / up |
| G | Go to page |
| + / − / Ctrl+0 | Zoom in / out / reset |
| B / W / H / S / A | Fit best / width / height / size / manual |
| R / Shift+R | Rotate clockwise / counter-clockwise |
| D | Double page |
| M | Manga mode |
| F, F11 / Esc | Fullscreen / leave fullscreen |
| F9 | Thumbnails |
| L | Magnifying lens |
| I | Hide all controls |
| Ctrl+D / Ctrl+B | Add bookmark / edit bookmarks |
| Ctrl+S | Slideshow |

Every key can be changed in Preferences.

## Files

Nira follows the XDG base directories:

- `~/.config/nira/settings.toml`: settings
- `~/.config/nira/keys.toml`: only the keys you have changed
- `~/.local/share/nira/`: reading history and bookmarks
- `~/.cache/nira/`: thumbnails

## Translations

Strings are [Fluent](https://projectfluent.org/) files in `i18n/<locale>/nira.ftl`. `en-US` is complete. A new locale can translate as many strings as it likes, and anything missing falls back to English.

## License

Nira is free software under the GNU General Public License, version 2 or (at your option) any later version. See [COPYING](COPYING).

Copyright © 2005–2026 the MComix/Comix developers, 2026 Rippa The Hutt, 2026 lokesh.
