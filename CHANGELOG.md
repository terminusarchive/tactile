# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.1.1] - 2026-10-13

### Security

- Nothing is shown while the session is locked, so keys typed at the lock screen no
  longer appear on the bar after unlocking.
- Keyboards are opened read-only.
- The stop commands signal only the user's own `tactile`.
- The README says what membership of the `input` group grants.

### Fixed

- The launcher entry now finds a `tactile` installed by cargo when `~/.cargo/bin` is
  not on the session's PATH.
- An argument that is not valid UTF-8, or a malformed keyboard layout reported by
  Hyprland, now gives an error instead of a crash.

## [0.1.0] - 2026-10-13

### Added

- On-screen overlay of pressed keys.
- App icon and launcher entry; the bar is limited to 60% of the monitor's width.
