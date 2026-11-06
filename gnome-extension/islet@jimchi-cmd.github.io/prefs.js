// Settings window for the Islet companion extension (GNOME 45+).

import Adw from 'gi://Adw';
import Gtk from 'gi://Gtk';
import Gio from 'gi://Gio';
import {ExtensionPreferences} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

export default class IsletPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const settings = this.getSettings();
        const page = new Adw.PreferencesPage();
        const group = new Adw.PreferencesGroup({
            title: 'Islet',
            description: 'These help the island on GNOME. Islet itself is set up in Islet Settings.',
        });
        page.add(group);

        const shortcut = new Adw.EntryRow({
            title: 'Shortcut to open or close the island, e.g. <Control><Alt>i (empty: none)',
            text: settings.get_strv('toggle-island')[0] ?? '',
        });
        shortcut.connect('changed', () => {
            const text = shortcut.text.trim();
            if (!text) {
                settings.set_strv('toggle-island', []);
                shortcut.remove_css_class('error');
                return;
            }
            const [ok] = Gtk.accelerator_parse(text);
            if (ok) {
                settings.set_strv('toggle-island', [text]);
                shortcut.remove_css_class('error');
            } else {
                shortcut.add_css_class('error');
            }
        });
        group.add(shortcut);

        const caps = new Adw.SwitchRow({
            title: 'Caps Lock pop-up',
            subtitle: 'Tell Islet when Caps Lock turns on or off',
        });
        settings.bind('caps-lock', caps, 'active', Gio.SettingsBindFlags.DEFAULT);
        group.add(caps);

        const clock = new Adw.SwitchRow({
            title: 'Hide the top bar clock',
            subtitle: 'The island sits over it at the top centre',
        });
        settings.bind('hide-clock', clock, 'active', Gio.SettingsBindFlags.DEFAULT);
        group.add(clock);

        const path = new Adw.EntryRow({
            title: 'Path to islet (empty: find it automatically)',
            text: settings.get_string('islet-path'),
        });
        path.connect('changed', () => settings.set_string('islet-path', path.text.trim()));
        group.add(path);

        window.add(page);
    }
}
