set -eu
output=$1
applet=$2
desktop_entry=$3
app_id=$4
fixtures=$5
click_x=$6
click_y=$7
symbolic_icon=$8
base=$(mktemp -d "${TMPDIR:-/tmp}/ferris-cosmic.XXXXXX")
xvfb=
cleanup() {
    if test -n "$xvfb"; then
        kill "$xvfb" 2>/dev/null || true
        wait "$xvfb" 2>/dev/null || true
    fi
    rm -rf -- "$base"
}
trap cleanup EXIT

mkdir -m 700 "$base/runtime" "$base/home" "$base/config" "$base/cache" "$base/bin" "$base/data"
mkdir -p "$output/checkpoints"
if ! test -f "$applet" || ! test -x "$applet"; then
    echo "Applet binary is not executable."
    exit 1
fi
if ! grep -q '^X-CosmicApplet=true$' "$desktop_entry"; then
    echo "Desktop entry is missing X-CosmicApplet=true."
    exit 1
fi
if ! grep -q '^Exec=' "$desktop_entry"; then
    echo "Desktop entry is missing Exec."
    exit 1
fi
if test -n "$symbolic_icon"; then
    for group in symbolic scalable; do
        icons="$base/data/icons/hicolor/$group/apps"
        mkdir -p "$icons"
        cp -- "$symbolic_icon" "$icons/$app_id-symbolic.svg"
    done
fi
Xvfb -displayfd 3 -screen 0 1280x800x24 -nolisten tcp 3>"$base/display" &
xvfb=$!
for _ in {1..40}; do
    if test -s "$base/display" || ! kill -0 "$xvfb" 2>/dev/null; then
        break
    fi
    sleep .1
done
if ! test -s "$base/display"; then
    echo "Xvfb failed to start a private display."
    exit 1
fi

env -i PATH="$base/bin:$fixtures:/usr/bin:/bin" HOME="$base/home" \
    XDG_CONFIG_HOME="$base/config" XDG_CACHE_HOME="$base/cache" \
    XDG_DATA_HOME="$base/data" XDG_RUNTIME_DIR="$base/runtime" \
    DISPLAY=":$(cat "$base/display")" COSMIC_BACKEND=winit LIBGL_ALWAYS_SOFTWARE=1 \
    FERRIS_APPLET_BINARY="$applet" FERRIS_NATIVE_TMP="$base" \
    /usr/bin/dbus-run-session -- /usr/bin/timeout 20s /bin/bash -s -- \
    "$output" "$desktop_entry" "$app_id" "$click_x" "$click_y" <<'NESTED'
set -eu
output=$1
desktop_entry=$2
app_id=$3
click_x=$4
click_y=$5

panel_config="$XDG_CONFIG_HOME/cosmic/com.system76.CosmicPanel"
cfg="$panel_config.Panel/v1"
mkdir -p "$panel_config/v1" "$cfg" "$XDG_DATA_HOME/applications"
printf '["Panel"]' > "$panel_config/v1/entries"
cp /usr/share/cosmic/com.system76.CosmicPanel.Panel/v1/* "$cfg/"
printf 'Some(["%s"])' "$app_id" > "$cfg/plugins_center"
printf 'None' > "$cfg/plugins_wings"
sed '0,/^Exec=/{s/^Exec=.*/Exec=ferris-applet-target/;}' "$desktop_entry" \
    > "$XDG_DATA_HOME/applications/$app_id.desktop"
cat > "$FERRIS_NATIVE_TMP/bin/ferris-applet-target" <<'APPLET'
#!/bin/sh
echo "$$" > "$FERRIS_NATIVE_TMP/applet.pid"
exec "$FERRIS_APPLET_BINARY"
APPLET
chmod +x "$FERRIS_NATIVE_TMP/bin/ferris-applet-target"

cosmic-comp >"$output/compositor.log" 2>&1 &
comp=$!
panel=
cleanup() {
    if test -n "$panel"; then
        kill "$panel" 2>/dev/null || true
    fi
    if test -s "$FERRIS_NATIVE_TMP/applet.pid"; then
        kill "$(cat "$FERRIS_NATIVE_TMP/applet.pid")" 2>/dev/null || true
    fi
    kill "$comp" 2>/dev/null || true
    wait "$comp" 2>/dev/null || true
}
trap cleanup EXIT
shopt -s nullglob
for _ in {1..40}; do
    sockets=("$XDG_RUNTIME_DIR"/wayland-*)
    if test "${#sockets[@]}" -gt 0 && test -S "${sockets[0]}"; then
        break
    fi
    sleep .1
done
if ! test "${#sockets[@]}" -gt 0 || ! test -S "${sockets[0]}"; then
    echo "Nested compositor did not create a Wayland socket."
    exit 1
fi
WAYLAND_DISPLAY="${sockets[0]##*/}" cosmic-panel >"$output/panel.log" 2>&1 &
panel=$!
for _ in {1..50}; do
    if test -s "$FERRIS_NATIVE_TMP/applet.pid" || ! kill -0 "$panel" 2>/dev/null; then
        break
    fi
    sleep .1
done
if ! test -s "$FERRIS_NATIVE_TMP/applet.pid"; then
    echo "The nested panel did not launch the applet."
    exit 1
fi
sleep 1
if ! kill -0 "$(cat "$FERRIS_NATIVE_TMP/applet.pid")" 2>/dev/null; then
    echo "The applet exited before the first checkpoint."
    exit 1
fi
pointer() {
    python3 - "$click_x" "$click_y" "$1" <<'PYTHON'
import sys
import ctypes as c

x11 = c.CDLL("libX11.so.6")
xt = c.CDLL("libXtst.so.6")
x11.XOpenDisplay.restype = c.c_void_p
display = c.c_void_p(x11.XOpenDisplay(None))
if not display:
    raise SystemExit("Cannot open the isolated X display")
x, y = map(int, sys.argv[1:3])
xt.XTestFakeMotionEvent(display, -1, x, y, 0)
if sys.argv[3] == "click":
    xt.XTestFakeButtonEvent(display, 1, 1, 0)
    xt.XTestFakeButtonEvent(display, 1, 0, 0)
x11.XSync(display, 0)
x11.XCloseDisplay(display)
PYTHON
}
screenshot() {
    ffmpeg -nostdin -hide_banner -loglevel error -f x11grab -video_size 1280x800 \
        -draw_mouse 0 -i "$DISPLAY" -frames:v 1 "$output/checkpoints/$1.png"
}
pointer move
sleep .25
screenshot closed
pointer click
sleep 1
if ! kill -0 "$(cat "$FERRIS_NATIVE_TMP/applet.pid")" 2>/dev/null; then
    echo "The applet exited after the panel click."
    exit 1
fi
screenshot open
NESTED
