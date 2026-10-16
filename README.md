# tactile

An on-screen overlay of the keys you press, for tachyon and Hyprland. Run one command and every
key appears as a cap on a translucent bar, which is handy for screencasts and demos.

## Requirements

- tachyon or Hyprland. tactile uses tachyon when its socket is present, and Hyprland when
  `HYPRLAND_INSTANCE_SIGNATURE` is set. On tachyon it asks `core.keyboard` for the layout,
  which needs a tachyon that has it; an older tachyon gets libxkbcommon's default layout.
- GTK4, gtk4-layer-shell and libxkbcommon. On Arch: `gtk4`, `gtk4-layer-shell`,
  `libxkbcommon`.
- To build with `cargo install`: Rust 1.92 or newer, pkg-config, and the development
  files for those three libraries. Arch's packages include them; other distributions
  ship them as separate development packages.
- Your user in the `input` group:

  ```sh
  sudo usermod -aG input $USER
  ```

  Log out and back in for it to take effect. See Security below.

## Security

Membership of the `input` group lets every program you run read all keystrokes from
every keyboard on the machine, including at the lock screen and in other sessions, and
inject key presses. That is true whether or not tactile is running, and it stays true
after tactile is removed. To leave the group, run `sudo gpasswd -d $USER input`, then
log out and back in.

Tactile reads the keys to draw them and does nothing else with them: none of what is typed is
written to disk, logged or sent anywhere, and nothing is shown while the session is locked.
The overlay does show everything you type, passwords included.

## Install

```sh
cargo install --locked tactile-keys
```

This installs the `tactile` command. `--locked` builds with the dependency versions that
were tested.

## Usage

```sh
tactile
```

The overlay appears on the monitor that has focus and runs until you stop it. Only one
runs at a time. Stop it with `pkill -x -u $(id -u) tactile`, which signals only your own
processes.

`--position` sets where the bar sits: `top-left`, `top`, `top-right`, `bottom-left`,
`bottom` (the default) or `bottom-right`.

```sh
tactile --position top-right
```

To toggle it with a key in `hyprland.lua`:

```lua
hl.bind("SUPER + K", hl.dsp.exec_cmd("pkill -x -u $(id -u) tactile || { PATH=$HOME/.cargo/bin:$PATH; exec tactile; }"))
```

On tachyon, bind it in `binds.toml`:

```toml
"$mod+k" = "core.spawn tactile"
```

That starts tactile; stop it with `pkill -x -u $(id -u) tactile`. Bind that command too
(for example `"$mod+shift+k" = "core.spawn pkill -x -u $(id -u) tactile"`) if you want a
key for it.

## What it shows

- A cap for each key pressed, lit while the key is held.
- A count (`×2`, `×3`) when a modifier or shortcut key repeats.
- Shift folded into the typed character: `H`, `!`.
- The bar clears after 2 seconds with no key pressed or held.

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
