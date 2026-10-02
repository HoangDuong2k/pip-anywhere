// D-Bus API (com.pipanywhere.Shell) used by the PiP Anywhere native host to pin the focused
// window above others and change its opacity. Wayland gives applications no way to do either.
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Config from 'resource:///org/gnome/shell/misc/config.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

/** Bump together with native/host.py and the extension's REQUIRED_HELPER_VERSION. */
const VERSION = 5;
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
    <method name="SetHoverReveal">
      <arg type="b" direction="in" name="enabled"/>
      <arg type="s" direction="out" name="json"/>
    </method>
    <method name="Diagnostics"><arg type="s" direction="out" name="json"/></method>
    <property name="Version" type="u" access="read"/>
  </interface>
</node>`;

const MIN_OPACITY = 0.2;
const TOP_LAYER = Meta.StackLayer?.TOP ?? 4;
const TITLE_CHARS = 60;
/** "Clear on hover": how often the pointer is checked, and how long it must stay away. */
const HOVER_POLL_MS = 100;
const HOVER_LEAVE_MS = 300;
const HOVER_FADE_MS = 150;
/** After an opacity change, show the chosen value this long even under the pointer. */
const PREVIEW_MS = 1500;

export default class PipAnywhereExtension extends Extension {
    enable() {
        /**
         * Translucent windows: MetaWindow -> {opacity, actor, handlers}. GNOME resets a window
         * actor's opacity to 255 at the end of its animations (minimise, unminimise, map, ...),
         * so we watch the actor and restore our value whenever that happens.
         */
        this._windows = new Map();
        /**
         * Windows pinned through PiP Anywhere: MetaWindow -> handler ids. Other code may unpin
         * them behind the user's back (Ubuntu's Tiling Assistant calls unmake_above() whenever
         * it tiles a window), so we pin them again until they are unpinned through PiP Anywhere.
         */
        this._pinned = new Map();
        /** Pending "before redraw" callbacks, keyed by `${kind}:${window id}`. */
        this._laters = new Map();
        /** What the extension had to repair, for Diagnostics(). */
        this._stats = {repins: 0, layerFixes: 0, events: []};
        /**
         * "Clear on hover": translucent windows turn fully opaque while the pointer is over them.
         * GNOME does not report the pointer entering other apps' windows, so while the option is
         * on and a translucent window exists, the pointer position is checked every 100 ms.
         */
        this._hoverReveal = false;
        this._hovered = null;
        this._hoverLeftAt = 0;
        this._hoverPollId = 0;
        this._previewUntil = 0;

        // A window's actor can be replaced (e.g. when an X11 window is remapped).
        this._mapId = global.window_manager.connect('map', (_wm, actor) => {
            const win = actor.meta_window;
            if (this._windows.has(win))
                this._watch(win);
        });
        this._restackedId = global.display.connect('restacked', () => this._checkPinnedLayers());

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
        this._stopHoverPolling();
        global.display.disconnect(this._restackedId);
        const laters = global.compositor.get_laters();
        this._laters.forEach(id => laters.remove(id));
        this._laters = null;
        // Pinned windows stay pinned (like "Always on Top" from the window menu); we only stop
        // re-pinning them.
        for (const win of [...this._pinned.keys()])
            this._unpin(win, false);
        this._pinned = null;
        // Leave windows as opaque as we found them.
        for (const win of [...this._windows.keys()]) {
            const actor = win.get_compositor_private();
            this._unwatch(win);
            if (actor)
                actor.opacity = 255;
        }
        this._windows = null;
    }

    get Version() {
        return VERSION;
    }

    GetFocused() {
        return JSON.stringify(this._describe(this._target()));
    }

    SetAbove(above) {
        const win = this._target();
        if (win) {
            if (above)
                this._pin(win);
            else
                this._unpin(win, true);
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
            // Let the user see the new value even if the pointer is over the window (e.g. on the
            // browser's own popup, or when using the keyboard shortcuts).
            this._previewUntil = GLib.get_monotonic_time() / 1000 + PREVIEW_MS;
            const actor = win.get_compositor_private();
            if (actor) {
                actor.remove_transition('opacity');
                actor.opacity = value >= 1 ? 255 : this._targetOpacity(win);
            }
        }
        return JSON.stringify(this._describe(win));
    }

    SetHoverReveal(enabled) {
        this._hoverReveal = !!enabled;
        if (!this._hoverReveal)
            this._setHovered(null);
        this._updateHoverPolling();
        return JSON.stringify(this._describe(this._target()));
    }

    /** Everything needed to understand "the window is pinned but something covers it". */
    Diagnostics() {
        const display = global.display;
        const short = s => (s ?? '').slice(0, TITLE_CHARS);
        const stack = display.sort_windows_by_stacking(display.list_all_windows()).reverse();
        const windowActors = global.window_group.get_children().filter(a => a.meta_window).reverse();
        const describeWindow = w => ({
            id: w.get_id(),
            title: short(w.get_title()),
            wm_class: w.get_wm_class() ?? '',
            type: w.get_window_type(),
            client: w.get_client_type() === Meta.WindowClientType.X11 ? 'x11' : 'wayland',
            layer: w.get_layer(),
            above: w.is_above(),
            pinned_by_us: this._pinned.has(w),
            maximized: w.is_maximized?.() ?? false,
            fullscreen: w.is_fullscreen(),
            minimized: w.minimized,
            workspace: w.is_on_all_workspaces() ? 'all' : w.get_workspace()?.index() ?? null,
            monitor: w.get_monitor(),
            opacity: w.get_compositor_private()?.opacity ?? null,
        });
        return JSON.stringify({
            version: VERSION,
            shell_version: Config.PACKAGE_VERSION,
            wayland: global.context?.get_compositor_type?.() === Meta.CompositorType?.WAYLAND,
            overview_visible: Main.overview.visible,
            active_workspace: global.workspace_manager.get_active_workspace_index(),
            extensions: Main.extensionManager.getUuids()
                .map(uuid => ({uuid, state: Main.extensionManager.lookup(uuid)?.state ?? null}))
                .filter(e => e.state === 1),
            focus: display.focus_window ? describeWindow(display.focus_window) : null,
            stats: this._stats,
            hover_reveal: this._hoverReveal,
            hovered: this._hovered?.get_wm_class() ?? null,
            // Top first. If a pinned window has a higher index than a window covering it here,
            // GNOME itself stacks it lower; if only `actors` disagrees, it is a drawing problem.
            stack: stack.map(describeWindow),
            actors: windowActors.map(a => ({id: a.meta_window.get_id(), wm_class: a.meta_window.get_wm_class() ?? ''})),
        });
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
            return {ok: false, error: 'No focused window.', version: VERSION};
        return {
            ok: true,
            version: VERSION,
            window: {
                id: win.get_id(),
                title: win.get_title() ?? '',
                wm_class: win.get_wm_class() ?? '',
                above: win.is_above(),
                opacity: this._windows.get(win)?.opacity ?? 1,
                hover_reveal: this._hoverReveal,
            },
        };
    }

    _record(kind, win, detail) {
        this._stats[kind === 'repin' ? 'repins' : 'layerFixes']++;
        this._stats.events.push({
            time: new Date().toISOString(),
            kind,
            wm_class: win.get_wm_class() ?? '',
            title: (win.get_title() ?? '').slice(0, TITLE_CHARS),
            detail,
        });
        this._stats.events.splice(0, this._stats.events.length - 20);
        console.log(`PiP Anywhere: ${kind} for "${win.get_title()}" (${detail})`);
    }

    // ---------------------------------------------------------------- always on top

    _pin(win) {
        if (!this._pinned.has(win)) {
            this._pinned.set(win, {
                aboveId: win.connect('notify::above', () => {
                    if (!win.is_above())
                        this._later(`pin:${win.get_id()}`, () => this._repin(win));
                }),
                unmanagedId: win.connect('unmanaged', () => this._unpin(win, false)),
            });
        }
        win.make_above();
    }

    _unpin(win, apply) {
        const entry = this._pinned.get(win);
        if (entry) {
            win.disconnect(entry.aboveId);
            win.disconnect(entry.unmanagedId);
            this._pinned.delete(win);
        }
        this._cancelLater(`pin:${win.get_id()}`);
        if (apply)
            win.unmake_above();
    }

    _repin(win) {
        if (this._pinned.has(win) && !win.is_above()) {
            win.make_above();
            this._record('repin', win, 'unpinned by another component');
        }
    }

    /** A pinned window should be in the top layer; if GNOME put it lower, recompute its layer. */
    _checkPinnedLayers() {
        for (const win of this._pinned.keys()) {
            if (win.is_above() && !win.minimized && win.get_layer() < TOP_LAYER) {
                this._later(`layer:${win.get_id()}`, () => {
                    if (!this._pinned.has(win) || win.get_layer() >= TOP_LAYER)
                        return;
                    const before = win.get_layer();
                    win.unmake_above(); // notify::above queues a repin before the next frame
                    win.make_above();
                    this._record('layer-fix', win, `layer ${before} -> ${win.get_layer()}`);
                });
            }
        }
    }

    // ---------------------------------------------------------------- opacity

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
        entry.opacityId = actor?.connect('notify::opacity',
            () => this._later(`opacity:${win.get_id()}`, () => this._restoreOpacity(win)));
        this._windows.set(win, entry);
        this._updateHoverPolling();
    }

    _unwatch(win) {
        const entry = this._windows.get(win);
        if (!entry)
            return;
        entry.actor?.disconnect(entry.opacityId);
        win.disconnect(entry.unmanagedId);
        this._windows.delete(win);
        this._cancelLater(`opacity:${win.get_id()}`);
        if (this._hovered === win)
            this._hovered = null;
        this._updateHoverPolling();
    }

    /** Restores our opacity once no animation is driving it. */
    _restoreOpacity(win) {
        const entry = this._windows.get(win);
        const actor = entry?.actor;
        // While an animation eases the opacity, let it run: it resets the value to 255 when
        // it finishes, which notifies us again.
        if (actor && !actor.get_transition('opacity')) {
            const target = this._targetOpacity(win);
            if (actor.opacity !== target)
                actor.opacity = target;
        }
    }

    /** Opacity (0-255) the actor should have right now. */
    _targetOpacity(win) {
        const previewing = GLib.get_monotonic_time() / 1000 < this._previewUntil;
        if (this._hoverReveal && this._hovered === win && !previewing)
            return 255;
        return Math.round((this._windows.get(win)?.opacity ?? 1) * 255);
    }

    // ---------------------------------------------------------------- clear on hover

    _updateHoverPolling() {
        const needed = this._hoverReveal && this._windows.size > 0;
        if (needed && !this._hoverPollId) {
            this._hoverPollId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, HOVER_POLL_MS, () => {
                this._checkHover();
                return GLib.SOURCE_CONTINUE;
            });
        } else if (!needed) {
            this._stopHoverPolling();
            this._setHovered(null);
        }
    }

    _stopHoverPolling() {
        if (this._hoverPollId) {
            GLib.source_remove(this._hoverPollId);
            this._hoverPollId = 0;
        }
    }

    /** The translucent window the pointer is over, if it is the topmost window there. */
    _windowUnderPointer() {
        const [x, y] = global.get_pointer();
        const workspace = global.workspace_manager.get_active_workspace();
        const stack = global.display.sort_windows_by_stacking(global.display.list_all_windows());
        for (let i = stack.length - 1; i >= 0; i--) {
            const win = stack[i];
            if (win.minimized || !win.located_on_workspace(workspace))
                continue;
            const r = win.get_frame_rect();
            if (x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height) {
                // Popups and menus of the window (e.g. the browser's extension popup) count too.
                const root = win.find_root_ancestor();
                return this._windows.has(root) ? root : null;
            }
        }
        return null;
    }

    _checkHover() {
        if (Main.overview.visible) {
            this._setHovered(null);
            return;
        }
        const win = this._windowUnderPointer();
        if (win) {
            this._hoverLeftAt = 0;
            this._setHovered(win);
            this._fadeTo(win); // e.g. once an opacity preview is over
        } else if (this._hovered) {
            // Wait a moment before fading back, so crossing an edge does not flicker.
            const now = GLib.get_monotonic_time() / 1000;
            this._hoverLeftAt ||= now;
            if (now - this._hoverLeftAt >= HOVER_LEAVE_MS)
                this._setHovered(null);
        }
    }

    _setHovered(win) {
        if (this._hovered === win)
            return;
        const previous = this._hovered;
        this._hovered = win;
        this._hoverLeftAt = 0;
        for (const w of [previous, win])
            this._fadeTo(w, true);
    }

    /** Eases `win` towards its target opacity unless it is already there (or `force`). */
    _fadeTo(win, force = false) {
        const actor = win?.get_compositor_private();
        if (!actor || !this._windows.has(win))
            return;
        const target = this._targetOpacity(win);
        if (!force && (actor.opacity === target || actor.get_transition('opacity')))
            return;
        actor.ease({opacity: target, duration: HOVER_FADE_MS, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
    }

    // ---------------------------------------------------------------- helpers

    /** Runs `fn` right before the next frame, at most once per key. */
    _later(key, fn) {
        if (this._laters.has(key))
            return;
        const id = global.compositor.get_laters().add(Meta.LaterType.BEFORE_REDRAW, () => {
            this._laters?.delete(key);
            fn();
            return GLib.SOURCE_REMOVE;
        });
        this._laters.set(key, id);
    }

    _cancelLater(key) {
        const id = this._laters.get(key);
        if (id !== undefined) {
            global.compositor.get_laters().remove(id);
            this._laters.delete(key);
        }
    }
}
