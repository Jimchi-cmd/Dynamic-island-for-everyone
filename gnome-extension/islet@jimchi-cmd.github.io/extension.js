// Islet companion for GNOME Shell 45+.
//
// Islet runs on GNOME through XWayland, which works, but GNOME doesn't let
// XWayland apps see global keys or the keyboard's lock state. This extension
// does those two things for it, by running Islet's own command line:
//   - a shortcut (default Ctrl+Alt+I) runs `islet toggle`;
//   - Caps Lock changes run `islet caps-lock on|off` (for the pop-up).
// Optionally it hides the top bar's clock, which the island covers.
// It makes no network requests and stores nothing but its own settings.

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const KEY_TOGGLE = 'toggle-island';

export default class IsletExtension extends Extension {
    enable() {
        this._settings = this.getSettings();

        Main.wm.addKeybinding(
            KEY_TOGGLE,
            this._settings,
            Meta.KeyBindingFlags.NONE,
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
            () => this._islet(['toggle'])
        );

        this._keymap = Clutter.get_default_backend().get_default_seat().get_keymap();
        this._caps = this._keymap.get_caps_lock_state();
        this._capsId = this._keymap.connect('state-changed', () => {
            const on = this._keymap.get_caps_lock_state();
            if (on === this._caps)
                return;
            this._caps = on;
            if (this._settings.get_boolean('caps-lock'))
                this._islet(['caps-lock', on ? 'on' : 'off']);
        });

        this._clockId = this._settings.connect('changed::hide-clock', () => this._syncClock());
        this._syncClock();
    }

    disable() {
        Main.wm.removeKeybinding(KEY_TOGGLE);
        if (this._keymap && this._capsId)
            this._keymap.disconnect(this._capsId);
        this._keymap = null;
        this._capsId = 0;
        if (this._settings && this._clockId)
            this._settings.disconnect(this._clockId);
        this._clockId = 0;
        this._showClock(true);
        this._settings = null;
    }

    _syncClock() {
        this._showClock(!this._settings.get_boolean('hide-clock'));
    }

    _showClock(visible) {
        const clock = Main.panel.statusArea.dateMenu?.container;
        if (clock)
            clock.visible = visible;
    }

    /** The islet program: the configured path, PATH, or where installers put it. */
    _binary() {
        const custom = this._settings.get_string('islet-path').trim();
        const candidates = [
            custom,
            GLib.find_program_in_path('islet'),
            '/usr/bin/islet',
            '/usr/local/bin/islet',
            `${GLib.get_home_dir()}/.local/bin/islet`,
        ];
        return candidates.find(p => p && GLib.file_test(p, GLib.FileTest.IS_EXECUTABLE)) ?? null;
    }

    _islet(args) {
        const bin = this._binary();
        if (!bin) {
            console.warn('Islet: the islet program was not found; set its path in the extension settings');
            return;
        }
        try {
            const proc = Gio.Subprocess.new(
                [bin, ...args],
                Gio.SubprocessFlags.STDOUT_SILENCE | Gio.SubprocessFlags.STDERR_SILENCE
            );
            // Reap it when it exits.
            proc.wait_async(null, null);
        } catch (e) {
            console.warn(`Islet: couldn't run islet: ${e.message}`);
        }
    }
}
