// D-Bus API (com.pipanywhere.Shell) used by the PiP Anywhere native host to pin the focused
// window above others and change its opacity. Wayland gives applications no way to do either.
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'com.pipanywhere.Shell';
const OBJECT_PATH = '/com/pipanywhere/Shell';
const IFACE = `
<node>
  <interface name="com.pipanywhere.Shell">
    <method name="GetFocused"><arg type="s" direction="out" name="json"/></method>
    <method name="SetAbove">
      <arg type="b" direction="in" name="above"/>
      <arg type="s" direction="out" name="json"/>
    </method>
    <method name="SetOpacity">
      <arg type="d" direction="in" name="opacity"/>
      <arg type="s" direction="out" name="json"/>
    </method>
    <property name="Version" type="u" access="read"/>
  </interface>
</node>`;

const MIN_OPACITY = 0.2;

export default class PipAnywhereExtension extends Extension {
    enable() {
        /**
         * Translucent windows: MetaWindow -> {opacity, actor, handlers}. GNOME resets a window
         * actor's opacity to 255 at the end of its animations (minimise, unminimise, map, ...),
         * so we watch the actor and restore our value whenever that happens.
         */
        this._windows = new Map();
        /** MetaWindow -> pending "before redraw" callback id. */
        this._laters = new Map();

        // A window's actor can be replaced (e.g. when an X11 window is remapped).
        this._mapId = global.window_manager.connect('map', (_wm, actor) => {
            const win = actor.meta_window;
            if (this._windows.has(win))
                this._watch(win);
        });

        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE, this);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
        this._nameId = Gio.bus_own_name_on_connection(
            Gio.DBus.session, BUS_NAME, Gio.BusNameOwnerFlags.NONE, null, null);
    }

    disable() {
        Gio.bus_unown_name(this._nameId);
        this._dbus.unexport();
        this._dbus = null;
        global.window_manager.disconnect(this._mapId);
        const laters = global.compositor.get_laters();
        this._laters.forEach(id => laters.remove(id));
        this._laters = null;
        // Leave windows as we found them.
        for (const win of [...this._windows.keys()]) {
            const actor = win.get_compositor_private();
            this._unwatch(win);
            if (actor)
                actor.opacity = 255;
        }
        this._windows = null;
    }

    get Version() {
        return 3;
    }

    GetFocused() {
        return JSON.stringify(this._describe(this._target()));
    }

    SetAbove(above) {
        const win = this._target();
        if (win) {
            if (above)
                win.make_above();
            else
                win.unmake_above();
        }
        return JSON.stringify(this._describe(win));
    }

    SetOpacity(opacity) {
        const win = this._target();
        if (win) {
            const value = Math.min(Math.max(opacity, MIN_OPACITY), 1);
            if (value >= 1) {
                this._unwatch(win);
            } else {
                this._watch(win);
                this._windows.get(win).opacity = value;
            }
            const actor = win.get_compositor_private();
            if (actor)
                actor.opacity = Math.round(value * 255);
        }
        return JSON.stringify(this._describe(win));
    }

    /**
     * The focused top-level window. When a popup or dialog of the browser has focus (e.g. the
     * extension popup on X11), act on the browser window that owns it.
     */
    _target() {
        return global.display.focus_window?.find_root_ancestor() ?? null;
    }

    _describe(win) {
        if (!win)
            return {ok: false, error: 'No focused window.'};
        return {
            ok: true,
            window: {
                id: win.get_id(),
                title: win.get_title() ?? '',
                wm_class: win.get_wm_class() ?? '',
                above: win.is_above(),
                opacity: this._windows.get(win)?.opacity ?? 1,
            },
        };
    }

    /** Starts (or refreshes) watching `win`'s actor for opacity changes. */
    _watch(win) {
        const actor = win.get_compositor_private();
        let entry = this._windows.get(win);
        if (entry && entry.actor === actor)
            return;
        if (entry)
            entry.actor?.disconnect(entry.opacityId);
        else
            entry = {opacity: 1, unmanagedId: win.connect('unmanaged', () => this._unwatch(win))};
        entry.actor = actor;
        entry.opacityId = actor?.connect('notify::opacity', () => this._queueRestore(win));
        this._windows.set(win, entry);
    }

    _unwatch(win) {
        const entry = this._windows.get(win);
        if (!entry)
            return;
        entry.actor?.disconnect(entry.opacityId);
        win.disconnect(entry.unmanagedId);
        this._windows.delete(win);
        const later = this._laters.get(win);
        if (later !== undefined) {
            global.compositor.get_laters().remove(later);
            this._laters.delete(win);
        }
    }

    /** Restores our opacity right before the next frame, once no animation is driving it. */
    _queueRestore(win) {
        if (this._laters.has(win))
            return;
        const id = global.compositor.get_laters().add(Meta.LaterType.BEFORE_REDRAW, () => {
            this._laters.delete(win);
            const entry = this._windows.get(win);
            const actor = entry?.actor;
            // While an animation eases the opacity, let it run: it resets the value to 255 when
            // it finishes, which notifies us again.
            if (actor && !actor.get_transition('opacity')) {
                const target = Math.round(entry.opacity * 255);
                if (actor.opacity !== target)
                    actor.opacity = target;
            }
            return GLib.SOURCE_REMOVE;
        });
        this._laters.set(win, id);
    }
}
