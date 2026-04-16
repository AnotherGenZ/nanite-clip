# Hotkey Architecture

This diagram reflects the current manual-clip hotkey flow after the KDE redesign.

## Backend Selection

```mermaid
flowchart TD
    A[App::configure_hotkeys] --> B[HotkeyManager::configure]
    B --> C{Manual clip enabled?}
    C -- No --> D[Disabled backend]
    C -- Yes --> E{Display server}

    E -- X11 or Unknown --> F[global-hotkey backend]
    F --> G[global-hotkey crate]

    E -- Wayland --> H{Desktop environment}
    H -- KDE Plasma --> I[Try KDE platform service backend]
    I --> J{Platform service ready?}
    J -- Yes --> K[nanite-clip-platform-service]
    K --> L[KGlobalAccel via Qt/KF6]
    J -- No --> M[Wayland portal backend]

    H -- Non-KDE --> M
    M --> N[xdg-desktop-portal GlobalShortcuts]

    D --> R[HotkeyManager]
    G --> R
    L --> R
    N --> R
```

## KDE Platform Service Flow

```mermaid
sequenceDiagram
    participant App as App
    participant HM as HotkeyManager
    participant PS as platform_service.rs
    participant Bin as nanite-clip-platform-service
    participant KDE as KGlobalAccel

    App->>HM: configure_hotkeys()
    HM->>HM: kde_shortcut_sequence()
    HM->>HM: clear_plasma_portal_shortcut_conflict()
    HM->>PS: start_plasma_manual_clip_hotkey(...)
    PS->>Bin: spawn process
    PS->>Bin: write JSON request on stdin
    Bin->>KDE: register action + apply shortcut
    KDE-->>Bin: active shortcut
    Bin-->>PS: {"event":"ready","binding_label":...}
    PS-->>HM: PlatformHotkeyServiceHandle
    HM-->>App: configured HotkeyManager

    KDE-->>Bin: QAction triggered
    Bin-->>PS: {"event":"activated"}
    PS-->>HM: PlatformHotkeyEvent::Activated
    HM-->>App: HotkeyEvent::Activated
```

## Notes

- Default KDE Wayland path is the platform service.
- If the KDE platform service is unavailable, KDE Wayland falls back to the same XDG portal backend used by other Wayland desktops.
- Non-KDE Wayland sessions go directly to the XDG portal backend.
- Non-Linux platforms use the same `global-hotkey` backend directly.
- The platform service boundary is intended to be reusable for future Linux-native integrations beyond hotkeys.
