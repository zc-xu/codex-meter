# Changelog

## 1.1.0 — 2026-07-31

### Added

- Dynamic menu bar/system tray quota ring that depletes clockwise with current usage
- Automatic next-renewal calculation from the last monthly recharge date

### Changed

- Menu bar popup now stays visible until explicitly hidden or toggled from the tray icon
- Larger typography and higher-contrast secondary text throughout the dashboard and settings
- Consistent left alignment for the compact footer metrics

### Fixed

- Settings and dashboard height restoration when switching modes or reopening from the tray
- Desktop-mode placement, dragging, and visible-area clamping
- Footer text clipping after the typography update
- Windows release builds no longer open an extra console window

## 1.0.0 — 2026-07-28

### Added

- Native-feeling macOS menu bar and Windows system tray modes
- Optional desktop-integrated mode with position memory
- Codex usage, reset time, remaining quota, task state, and Token metrics
- Theme, opacity, refresh interval, renewal date, launch-at-login, and fallback Codex path settings

### Fixed

- Window recovery after hiding or closing
- Retina and multi-display placement clamping
- Desktop-mode dragging and mode-specific window sizing
- Transparent system tray icon and compact settings layout

### Distribution notes

- macOS builds use ad-hoc signing and are not notarized
- Windows installers are not commercially code-signed
