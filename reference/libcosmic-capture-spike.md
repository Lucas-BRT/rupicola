# Capture a hosted COSMIC applet

`capture-cosmic` runs an existing Linux applet under `cosmic-panel` on a nested `cosmic-comp` using its winit backend over a private Xvfb display. The runner uses a private D-Bus session, temporary XDG paths and a panel with one centered applet. It captures before and after sending one pointer click. The applet's binary and desktop entry stay in the target checkout.

The runner needs the distribution's COSMIC compositor, panel and default panel configuration, plus Xvfb, D-Bus, FFmpeg with x11grab, Python 3 and XTest. Supply a desktop entry with `X-CosmicApplet=true`. Use a standalone executable and place mock commands needed by the test in the fixture directory on the private executable search path.

## Reproduce the Tailscale applet check

Point `APPLET` to the checkout with the existing release executable. This mock provides a single connected device, no peers and no write operations.

```bash
APPLET=/absolute/path/to/tailscale-cosmic-applet
fixtures=$(mktemp -d)
cat > "$fixtures/tailscale" <<'EOF_FIXTURE'
#!/bin/sh
case "$*" in
    'status --json')
        printf '%s\n' '{"BackendState":"Running","Self":{"HostName":"fixture","DNSName":"fixture.invalid.","TailscaleIPs":["100.64.0.2"]},"Peer":{}}'
        ;;
    *) exit 1 ;;
esac
EOF_FIXTURE
chmod +x "$fixtures/tailscale"

rupicola capture-cosmic \
  --applet "$APPLET/target/release/tailscale-cosmic-applet" \
  --desktop-entry "$APPLET/resources/app.desktop" \
  --app-id io.github.lucasbrt.TailscaleCosmicApplet \
  --fixtures "$fixtures" \
  --symbolic-icon "$APPLET/resources/icon-symbolic.svg" \
  --project-id tailscale-applet --project-label 'Tailscale applet' \
  --click 640 18 --out .rupicola/captures/cosmic-fixture
```

Open `checkpoints/closed.png` and `checkpoints/open.png` and record your observation with `capture-review`. The manifest describes the isolated renderer and any blocker. Logs stay in the run directory. A complete capture establishes that both frames were saved. Inspect the images to confirm the click opened the popup.

The verified run showed the popup anchored below the panel, the **Connected** state, the fixture device, and zero peers. The optional symbolic SVG supplied the panel button glyph. The environment lacked DRM and PipeWire. The compositor logged EGL fallback warnings and still produced both images. GPU rendering, real compositor pacing, additional applet states, keyboard interaction and native animation capture remain unverified. Use [visual verification](visual-verification.md) when an app already exposes an offscreen renderer for motion.
