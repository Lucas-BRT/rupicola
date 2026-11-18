# Platform convention tactics

Shared rules: [common](../common.md), [index](index.md). Platform notes: [linux](../platforms/linux.md), [windows](../platforms/windows.md), [macos](../platforms/macos.md), [webview](../environments/webview.md). Follow the project's target and the OS convention before inventing a pattern.

### Follow the system theme
Use when: any desktop app. Do: read light or dark, accent color and contrast preferences from the platform (libadwaita style manager on GNOME, the COSMIC theme, Windows and macOS appearance settings, `prefers-color-scheme` on the web) and update live. Avoid: a theme chosen once at launch.

### Window chrome
Use when: custom or native title bars. Do: keep window controls, a drag region and a title; on GNOME use a header bar with the primary menu; on macOS keep the traffic lights and toolbar conventions; on Windows keep caption buttons and snap behavior. Avoid: a custom title bar that breaks dragging, double-click maximize or controls.

### Menus and application commands
Use when: command-rich apps. Do: macOS uses the global menu bar with standard items (About, Settings with Cmd+comma, Hide, Quit with Cmd+Q); Windows and KDE favor in-window menu bars or toolbars; GNOME favors a primary menu in the header bar and a keyboard shortcuts window. Avoid: putting the same commands in a different place on each OS without reason.

### Shortcuts
Use when: any shortcut. Do: Ctrl on Linux and Windows maps to Cmd on macOS; use standard keys (copy, paste, undo, find, save, close, quit, preferences); list them in menus and a help view. Avoid: overriding system-reserved shortcuts and single-letter global shortcuts.

### File dialogs and portals
Use when: open, save, drag from the file manager. Do: use the native dialog (xdg-desktop-portal on Linux, native dialogs on Windows and macOS), remember the last folder, and support drag and drop onto the window. Avoid: a custom file browser.

### Notifications and tray
Use when: background activity. Do: use the platform notification service with actions and grouping; treat tray icons as optional, since stock GNOME does not show them by default. Avoid: requiring a tray to use the app.

### Window state and multiple windows
Use when: apps reopened daily. Do: restore size, position and maximized state, validate the position against available monitors, and support multiple windows where the workflow needs it. Avoid: off-screen windows after a monitor change.

### Scaling and high DPI
Use when: all desktop targets. Do: support fractional scaling, render vector assets and icons at the scale factor, test at 100%, 150% and 200%. Avoid: blurry bitmaps and fixed pixel sizes for text.

### Pointer and touch
Use when: hybrid devices. Do: support mouse, keyboard and touch, with target sizes suited to touch where touch is a target and no hover-only actions. Avoid: assuming a mouse.

### Clipboard and drag-drop
Use when: moving data. Do: support standard copy and paste with rich and plain forms, drop from the OS, and drag out where it makes sense. Avoid: copy that loses formatting silently.

### System integration
Use when: the app fits a larger workflow. Do: open files from the file manager, register file types where relevant, respect the default apps and the system language, and expose the usual app actions (recent files, jump lists, desktop actions). Avoid: reinventing system services.

### Web conventions
Use when: web frontends. Do: links are links and buttons are buttons, URLs reflect state, the back button works, forms support autofill, text is selectable, and right-click is not hijacked. Avoid: single-page apps that break back and deep links.

### Webview conventions (Tauri, Dioxus desktop)
Use when: embedding a web UI in a desktop shell. Do: remember the system webview engine differs per OS (WebKitGTK, WebView2, WKWebView) so test each; keep native menus, shortcuts and dialogs for desktop feel; make non-content text unselectable only where it behaves like chrome; set a drag region for custom titles. Avoid: shipping a website in a window with browser behaviors such as pinch zoom or default context menus unconsidered.

### Performance expectations
Use when: any target. Do: start fast, never block the UI thread, show something within a second, and measure on the slowest supported machine. Avoid: splash screens that hide slow startup.
