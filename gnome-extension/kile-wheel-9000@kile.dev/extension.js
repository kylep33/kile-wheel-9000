// The launcher's window into GNOME Shell. Wayland gives an ordinary app no window list,
// no focus control, no global pointer and no always-on-top surface, so the shell does
// those on its behalf over D-Bus. The launcher draws the wheel itself and hands the
// pixels over; this only puts them on screen.

import Clutter from 'gi://Clutter';
import Cogl from 'gi://Cogl';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

// bumped whenever the interface changes, so the launcher can say "update the extension"
const PROTOCOL_VERSION = 1;

const OBJECT_PATH = '/dev/kile/Wheel9000';

// the launcher aims by the pointer, so it is reported about once a frame while shown
const POINTER_POLL_MS = 8;

// one bytes-per-pixel figure for the premultiplied RGBA frames the launcher writes
const BYTES_PER_PIXEL = 4;

const INTERFACE_XML = `
<node>
  <interface name="dev.kile.Wheel9000">
    <method name="Version">
      <arg type="u" direction="out" name="version"/>
    </method>
    <method name="ListWindows">
      <arg type="a(tsssu)" direction="out" name="windows"/>
    </method>
    <method name="FocusedWindow">
      <arg type="t" direction="out" name="id"/>
    </method>
    <method name="Activate">
      <arg type="t" direction="in" name="id"/>
    </method>
    <method name="Minimize">
      <arg type="t" direction="in" name="id"/>
    </method>
    <method name="FocusIsFullscreen">
      <arg type="b" direction="out" name="fullscreen"/>
    </method>
    <method name="Monitors">
      <arg type="a(iiiiiiiidb)" direction="out" name="monitors"/>
    </method>
    <method name="CurrentMonitor">
      <arg type="i" direction="out" name="index"/>
    </method>
    <method name="Pointer">
      <arg type="i" direction="out" name="x"/>
      <arg type="i" direction="out" name="y"/>
    </method>
    <method name="WarpPointer">
      <arg type="i" direction="in" name="x"/>
      <arg type="i" direction="in" name="y"/>
    </method>
    <method name="ShowFrame">
      <arg type="s" direction="in" name="path"/>
      <arg type="i" direction="in" name="x"/>
      <arg type="i" direction="in" name="y"/>
      <arg type="i" direction="in" name="width"/>
      <arg type="i" direction="in" name="height"/>
      <arg type="i" direction="in" name="pixel_width"/>
      <arg type="i" direction="in" name="pixel_height"/>
    </method>
    <method name="HideFrame"/>
    <method name="SetClipboard">
      <arg type="s" direction="in" name="text"/>
    </method>
    <method name="GetClipboard">
      <arg type="s" direction="out" name="text"/>
    </method>
    <signal name="WindowsChanged"/>
    <signal name="PointerMoved">
      <arg type="i" name="x"/>
      <arg type="i" name="y"/>
    </signal>
  </interface>
</node>`;

class Bridge {
    constructor() {
        this._tracker = Shell.WindowTracker.get_default();
        this._windowSignals = new Map();
        this._displaySignals = [];
        this._changePending = 0;
        this._pointerSource = 0;
        this._lastPointer = null;
        this._actor = null;

        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, this);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);

        const display = global.display;
        this._displaySignals.push(
            display.connect('window-created', (_display, window) => {
                this._watchWindow(window);
                this._queueChange();
            }),
            display.connect('notify::focus-window', () => this._queueChange()));
        for (const window of this._normalWindows())
            this._watchWindow(window);
    }

    destroy() {
        this._stopPointer();
        if (this._changePending)
            GLib.source_remove(this._changePending);
        for (const id of this._displaySignals)
            global.display.disconnect(id);
        for (const [window, ids] of this._windowSignals) {
            for (const id of ids)
                window.disconnect(id);
        }
        this._windowSignals.clear();
        this._actor?.destroy();
        this._actor = null;
        this._dbus.unexport();
    }

    Version() {
        return PROTOCOL_VERSION;
    }

    ListWindows() {
        return this._normalWindows().map(window => {
            const app = this._tracker.get_window_app(window);
            const appId = app?.get_id() ?? `window:${window.get_wm_class() ?? 'unknown'}`;
            const appName = app?.get_name() ?? '';
            return [window.get_id(), appId, appName, window.get_title() ?? '', window.get_pid()];
        });
    }

    FocusedWindow() {
        return global.display.focus_window?.get_id() ?? 0;
    }

    Activate(id) {
        const window = this._find(id);
        if (window)
            Main.activateWindow(window, global.get_current_time());
    }

    Minimize(id) {
        this._find(id)?.minimize();
    }

    FocusIsFullscreen() {
        return global.display.focus_window?.is_fullscreen() ?? false;
    }

    Monitors() {
        const display = global.display;
        const primary = display.get_primary_monitor();
        // the integer UI scale; fractional scaling already lays the stage out in
        // logical pixels, so it needs no further factor here
        const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor;
        const monitors = [];
        for (let index = 0; index < display.get_n_monitors(); index++) {
            const bounds = display.get_monitor_geometry(index);
            const work = Main.layoutManager.getWorkAreaForMonitor(index);
            monitors.push([
                bounds.x, bounds.y, bounds.width, bounds.height,
                work.x, work.y, work.width, work.height,
                scale, index === primary,
            ]);
        }
        return monitors;
    }

    CurrentMonitor() {
        return global.display.get_current_monitor();
    }

    Pointer() {
        const [x, y] = global.get_pointer();
        return [x, y];
    }

    WarpPointer(x, y) {
        Clutter.get_default_backend().get_default_seat().warp_pointer(x, y);
    }

    ShowFrame(path, x, y, width, height, pixelWidth, pixelHeight) {
        const [, data] = GLib.file_get_contents(path);
        const image = St.ImageContent.new_with_preferred_size(width, height);
        const context = global.stage.context.get_backend().get_cogl_context();
        image.set_bytes(
            context, new GLib.Bytes(data), Cogl.PixelFormat.RGBA_8888_PRE,
            pixelWidth, pixelHeight, pixelWidth * BYTES_PER_PIXEL);

        if (!this._actor) {
            // not reactive: every click goes to the launcher's own input grab, and
            // nothing here may ever take focus from the app underneath
            this._actor = new Clutter.Actor({reactive: false});
            Main.uiGroup.add_child(this._actor);
        }
        this._actor.set_content(image);
        this._actor.set_position(x, y);
        this._actor.set_size(width, height);
        Main.uiGroup.set_child_above_sibling(this._actor, null);
        this._actor.show();
        this._startPointer();
    }

    HideFrame() {
        this._stopPointer();
        this._actor?.hide();
    }

    SetClipboard(text) {
        St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, text);
    }

    GetClipboardAsync(_params, invocation) {
        St.Clipboard.get_default().get_text(St.ClipboardType.CLIPBOARD, (_clipboard, text) => {
            invocation.return_value(new GLib.Variant('(s)', [text ?? '']));
        });
    }

    _normalWindows() {
        return global.display.get_tab_list(Meta.TabList.NORMAL_ALL, null);
    }

    _find(id) {
        return this._normalWindows().find(window => window.get_id() === Number(id)) ?? null;
    }

    _watchWindow(window) {
        if (this._windowSignals.has(window))
            return;
        const ids = [
            window.connect('notify::title', () => this._queueChange()),
            window.connect('notify::skip-taskbar', () => this._queueChange()),
            window.connect('unmanaged', () => {
                this._windowSignals.delete(window);
                this._queueChange();
            }),
        ];
        this._windowSignals.set(window, ids);
    }

    // a burst of changes, like a browser opening its tabs, becomes one signal
    _queueChange() {
        if (this._changePending)
            return;
        this._changePending = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            this._changePending = 0;
            this._dbus.emit_signal('WindowsChanged', null);
            return GLib.SOURCE_REMOVE;
        });
    }

    _startPointer() {
        if (this._pointerSource)
            return;
        this._pointerSource = GLib.timeout_add(GLib.PRIORITY_HIGH, POINTER_POLL_MS, () => {
            const [x, y] = global.get_pointer();
            if (this._lastPointer?.[0] !== x || this._lastPointer?.[1] !== y) {
                this._lastPointer = [x, y];
                this._dbus.emit_signal('PointerMoved', new GLib.Variant('(ii)', [x, y]));
            }
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopPointer() {
        if (!this._pointerSource)
            return;
        GLib.source_remove(this._pointerSource);
        this._pointerSource = 0;
        this._lastPointer = null;
    }
}

export default class KileWheelExtension extends Extension {
    enable() {
        this._bridge = new Bridge();
    }

    disable() {
        this._bridge?.destroy();
        this._bridge = null;
    }
}
