# tactile

An on-screen overlay of the keys you press, for Hyprland. Run one command and every
key appears as a cap on a translucent bar, which is handy for screencasts and demos.

## Requirements

- Hyprland.
- GTK4, gtk4-layer-shell and libxkbcommon. On Arch: `gtk4`, `gtk4-layer-shell`,
  `libxkbcommon`.
- To build with `cargo install`: Rust 1.92 or newer, pkg-config, and the development
  files for those three libraries. Arch's packages include them; other distributions
  ship them as separate development packages.
- Your user in the `input` group:

  ```sh
  sudo usermod -aG input $USER
  ```

  Log out and back in for it to take effect.

## Install

```sh
cargo install tactile-keys
```

This installs the `tactile` command.

## Usage

```sh
tactile
```

The overlay appears on the monitor that has focus and runs until you stop it. Only one
runs at a time. Stop it with `pkill -x tactile`.

`--position` sets where the bar sits: `top-left`, `top`, `top-right`, `bottom-left`,
`bottom` (the default) or `bottom-right`.

```sh
tactile --position top-right
```

To toggle it with a key in `hyprland.conf`:

```
bind = SUPER, K, exec, pkill -x tactile || tactile
```

## What it shows

- A cap for each key pressed, lit while the key is held.
- A count (`×2`, `×3`) when a modifier or shortcut key repeats.
- Shift folded into the typed character: `H`, `!`.
- The bar clears after 2 seconds with no key pressed or held.

It shows everything you type, passwords included.

## Launcher entry

Cargo installs only the command. To add tactile to your application launcher, copy the
desktop entry and the icon from this repository:

```sh
mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
cp assets/tactile.desktop ~/.local/share/applications/
cp assets/tactile.svg ~/.local/share/icons/hicolor/scalable/apps/
```

Activating the entry starts the overlay; activating it again stops it. The entry looks
for `tactile` in `~/.cargo/bin` as well as on your PATH.

## Licence

MIT. See [LICENSE](LICENSE).
