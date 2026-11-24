# Visual verification

Shared rules: [common](common.md), [verification](verification.md). Code that compiles says nothing about how an interface looks. Research on coding agents finds that rendered feedback improves GUI code (UI2Code^N, Coding with Eyes, RecreationWorld, WebGen-Agent), so look at the result whenever a capture is possible.

## Safety first

Do not open windows on the user's real desktop unless they agree. Use a headless or nested display (Xvfb, a nested or headless Wayland compositor, a toolkit's own test renderer, a browser in headless mode) or ask. Report plainly when no capture was possible: "visually unverified" is a valid and honest result.

## The loop

1. Build the target. Resolve what is missing (display libraries, a headless backend) before claiming anything.
2. Capture a matrix, not one screenshot: the changed state plus empty, loading, error, long text; light and dark; 100% and 200% text; the narrowest and a wide window; a right-to-left locale when localized. Exercise the app or its headless harness across those states, themes, text scales, and viewport sizes via server-managed or external capture tooling.
3. Combine checkpoints into contact sheets or inspect single images for detail. Look at hierarchy, alignment, spacing rhythm, clipping, contrast and state coverage, then write findings with [critique](critique.md).
4. Measure what can be measured: inspect extracted UI/DOM bounds (target size, contrast, clipping, overflow, sibling collisions) and palette colors against tokens.
5. Compare with the previous state (same viewport, theme, and state): check pixel diffs (`Split Slider`, `Side by Side`, or `Pixel Diff` in the panel); unexpected changes outside the edited area are regressions to explain.
6. For motion, prefer the two-tier capture strategy below: render frames into RAM at deterministic timestamps (Tier 1) and inspect sampled PNG frames plus an optional CFR MP4 (`ffmpeg`); also capture with reduced motion on. Check active range, repeated frames (stalls), and abrupt steps (jumps); a change confined to one or two frames is a cut, not a tween. Confirm real compositor pacing in Tier 2 when needed.
7. Fix, recapture, and stop when the evidence meets the acceptance condition.

For an HTTP(S) preview, use [headless capture scenarios](capture-scenarios.md) for scripted multi-step flows.

## Two-tier capture strategy

- **Tier 1 — Offscreen RAM rendering (default for iterative dev and animation checks):** render view trees directly into RAM pixel buffers with zero window/display interference (e.g., `iced_tiny_skia` into `tiny_skia::Pixmap`, `egui_kittest`, or Slint's software renderer) at deterministic timestamps (`0 ms`, `16.67 ms`, `33.33 ms`, ...) to produce lossless PNGs, an optional CFR MP4 (`libx264`, `yuv420p`, even-dimension padding) via `ffmpeg`, and per-frame CPU rasterization timings (`timing.json`). For CPU-rasterized dev builds (`iced_tiny_skia`), set `[profile.dev.package."*"] opt-level = 3` in `Cargo.toml` to avoid ~14 fps unoptimized dev raster bottlenecks.
- **Tier 2 — Real-compositor verification (final check when compositor behavior matters):** use when verifying Wayland `xdg_popup`, `autosize`, `cosmic-panel` applets (`X_PRIVILEGED_WAYLAND_SOCKET`), or real 60 Hz `wl_callback` pacing. On Wayland desktops such as Pop!_OS COSMIC (where headless Wayland compositors are not preinstalled, `xvfb-run` fails with `softbuffer` pixel-format errors, and `cosmic-screenshot` takes ~50–100 ms/frame over D-Bus/portal), record via PipeWire `org.freedesktop.portal.ScreenCast` v5 with `persist_mode = 2` (`restore_token` — single one-time user approval, zero dialogs on subsequent captures) using `gstreamer1.0-pipewire` + `ffmpeg` buffered in RAM (`/dev/shm`), rather than Xvfb or repeated portal screenshots.

**Motion capture rules:**
- Use Tier 1 offscreen rendering during iterative development whenever you add or tune an animation, transition, hover/press feedback, or reveal — it runs headlessly in RAM with deterministic timestamps and per-frame CPU raster timing, never disturbs the user's desktop, and avoids Xvfb/`softbuffer` failures.
- Use `ffmpeg` (`-framerate 60 -c:v libx264 -pix_fmt yuv420p`) when you have a PNG frame sequence and want a playable CFR MP4 to review pacing and smoothness visually.
- Do not rely on MP4 output alone for pixel-exact regression checks (`yuv420p` is lossy): always keep lossless PNGs for frame-level inspection and pixel diffs.
- Do not use Tier 1 offscreen rendering to claim compositor behavior (`xdg_popup` placement, `autosize`, panel applets, or hardware GPU/compositor 60 Hz pacing): verify those in Tier 2.

## UI tree audit metrics

When a toolkit helper dumps window size and node geometry as JSON (`id`, `role`, `x`, `y`, `w`, `h`, plus optional `name`, `interactive`, `fg`, `bg`, `font_px`, `bold`, `clipped`, `parent`, `kind`):

```json
{"window": {"w": 800, "h": 600},
 "nodes": [{"id": "save", "role": "button", "name": "Save", "x": 10, "y": 10, "w": 80, "h": 32,
            "interactive": true, "fg": "#000000", "bg": "#ffffff", "font_px": 14, "bold": false,
            "clipped": false, "parent": "toolbar", "kind": "text"}]}
```

Check target size (24 x 24 px), missing names, contrast (4.5:1, 3:1 for large text, icons and boundaries), clipping, off-window nodes, overlapping interactive nodes and sibling left edges that miss by 1 to 3 px. Findings are measurements of that data, not proof the app is accessible.

Where the tree comes from (confirm the call for the pinned version): a browser script walking `getBoundingClientRect`, computed styles and the accessibility tree; AccessKit trees in egui and Slint; AT-SPI on Linux; UI Automation on Windows; NSAccessibility on macOS; or the toolkit's own widget bounds.

## Capture recipes by toolkit

Except where marked verified, these are the usual routes, not verified here; confirm names and features for the pinned version, and prefer the project's existing test setup.

- Web: a headless browser or browser tools with viewport size, `prefers-color-scheme`, `prefers-reduced-motion` and device scale emulated.
- egui (verified in this repository's example): `egui_kittest` with the `wgpu` and `snapshot` features renders offscreen on software Vulkan, with no window and no display; see the egui module for the command and pitfalls.
- Slint (verified on a virtual X display, see below): run the app and capture the display; the software renderer into a pixel buffer is another route.
- Iced (verified in this repository's example): `iced_test`'s `Simulator` renders views on software Vulkan with no window, from state you set directly; see the Iced module. libcosmic (verified on a virtual X display with software Vulkan, see below): `iced_test` does not cover its widgets and runtime.
- GTK4 and libadwaita (verified on a virtual X display, see below): the Cairo renderer, then a display capture.
- GPUI (verified on a virtual X display with software Vulkan, see below).
- Any native app: a nested or headless Wayland compositor with a capture tool, or Xvfb with an X capture tool.

## Reference intake and fidelity

When the user supplies a screenshot or a design (an exported image, or a frame fetched through a design tool), treat it as the reference. Sample its palette and surface roles, read type and spacing off the image, and propose tokens before coding. After building, render the same state at the same size and compare against the reference (side-by-side and pixel diff): the changed fraction and region are a fidelity measure, and large changed regions are the next fixes. Fonts and anti-aliasing leave small subpixel differences; focus on layout, contrast, and structural hierarchy rather than chasing subpixel noise.

## Running a native app on a virtual display

Verified on Linux with Xvfb for GTK4, Slint, GPUI and libcosmic: start `Xvfb :99 -screen 0 1280x1024x24`, then run the app with `env -u WAYLAND_DISPLAY DISPLAY=:99` (toolkits prefer Wayland when `WAYLAND_DISPLAY` is set, and the window would open on the real desktop), plus `GDK_BACKEND=x11 GSK_RENDERER=cairo` for GTK, `WINIT_UNIX_BACKEND=x11` for winit toolkits and `VK_ICD_FILENAMES=<lavapipe icd>` for Vulkan toolkits. Capture with Pillow's `ImageGrab.grab(xdisplay=':99')`; send input with the XTest extension (for example from a short script using `ctypes` and libXtst) or `xdotool` if installed. Without a window manager, windows open at the origin or centered, nothing gives them input focus (set it yourself before sending keys), and popovers lack transparency. Screen grabs take roughly 100 ms each, so grabbed sequences see only about ten frames per second: a "jump" or "stall" can be a sampling artifact, so confirm with a faster capture before acting on it. A toolkit's own offscreen renderer (egui_kittest, iced_test) has no such limit.

## Evidence labels

Report each claim as measured (a tool number), seen (visible in a named image), opinion (a design judgment) or unverified (not checked, with the missing prerequisite). A model's judgment of a still image tracks experts on visual qualities such as aesthetics, but not on usability that depends on interaction, so say "needs an interactive check" for flows, focus order, timing and input.
