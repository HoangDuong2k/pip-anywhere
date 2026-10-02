// Test-only probe: reads and pokes what the compositor does to the focused window.
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const IFACE = `
<node>
  <interface name="com.pipanywhere.Test">
    <method name="Probe"><arg type="s" direction="out" name="json"/></method>
    <method name="HideOverview"/>
    <method name="Minimize"/>
    <method name="Unminimize"/>
    <method name="Debug"><arg type="s" direction="out" name="json"/></method>
    <method name="Animating"><arg type="b" direction="out" name="animating"/></method>
    <method name="Animations"><arg type="s" direction="out" name="json"/></method>
    <method name="UnmakeAbove"/>
    <method name="MovePointer"><arg type="d" direction="in" name="x"/><arg type="d" direction="in" name="y"/></method>
    <method name="WindowCenter"><arg type="s" direction="out" name="json"/></method>
    <method name="ActivateTest"/>
    <method name="ActivateOther"><arg type="b" direction="out" name="found"/></method>
    <method name="ForceActorOpacity"><arg type="i" direction="in" name="value"/></method>
  </interface>
</node>`;

export default class TestProbe extends Extension {
    enable() {
        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE, this);
        this._dbus.export(Gio.DBus.session, '/com/pipanywhere/Test');
        this._nameId = Gio.bus_own_name_on_connection(
            Gio.DBus.session, 'com.pipanywhere.Test', Gio.BusNameOwnerFlags.NONE, null, null);
        // A virtual mouse, to test "Clear on hover" with real pointer motion.
        this._pointer = Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    }

    disable() {
        Gio.bus_unown_name(this._nameId);
        this._dbus.unexport();
    }

    /** The test window: the first focused window, remembered so it can be probed while minimised. */
    _window() {
        this._testWindow ??= global.display.focus_window;
        return this._testWindow ?? null;
    }

    Probe() {
        const win = this._window();
        return JSON.stringify(win ? {
            title: win.get_title(),
            above: win.is_above(),
            minimized: win.minimized,
            actorOpacity: win.get_compositor_private()?.opacity ?? null,
        } : null);
    }

    Animations() {
        const actor = this._window()?.get_compositor_private();
        return JSON.stringify({
            enable: St.Settings.get().enable_animations,
            overview: Main.overview.visible,
            texture: !!actor?.get_texture(),
        });
    }

    HideOverview() {
        Main.overview.hide();
    }

    Minimize() {
        this._window().minimize();
    }

    Unminimize() {
        const win = this._window();
        win.unminimize();
        win.activate(global.get_current_time());
    }

    Debug() {
        const win = this._window();
        const actor = win?.get_compositor_private();
        return JSON.stringify({
            title: win?.get_title(), minimized: win?.minimized, type: win?.get_window_type(),
            client: win?.get_client_type(), visible: actor?.visible, opacity: actor?.opacity,
            scale: actor?.scale_x, minimizing: Main.wm._minimizing?.size,
            skipped: Main.wm._skippedActors?.has(actor), shouldAnimate: Main.wm._shouldAnimate(),
            texture: !!actor?.get_texture(),
        });
    }

    /** Whether a GNOME window animation is running, i.e. whether the test is realistic. */
    Animating() {
        const actor = this._window()?.get_compositor_private();
        return !!(actor?.get_transition('scale-x') || actor?.get_transition('opacity'));
    }

    MovePointer(x, y) {
        this._pointer.notify_absolute_motion(GLib.get_monotonic_time(), x, y);
    }

    WindowCenter() {
        const r = this._window().get_frame_rect();
        return JSON.stringify([r.x + r.width / 2, r.y + r.height / 2]);
    }

    // Simulates Ubuntu's Tiling Assistant, which unpins a window whenever it tiles it.
    UnmakeAbove() {
        this._window().unmake_above();
    }

    ActivateTest() {
        this._window().activate(global.get_current_time());
    }

    /** Focuses another normal window, like the user clicking a different app. */
    ActivateOther() {
        const other = global.display.list_all_windows().find(w =>
            w !== this._window() && w.get_window_type() === 0);
        other?.activate(global.get_current_time());
        return !!other;
    }

    // Simulates any other Shell code resetting the opacity (like _unminimizeWindowDone does).
    ForceActorOpacity(value) {
        this._window().get_compositor_private().set_opacity(value);
    }
}
