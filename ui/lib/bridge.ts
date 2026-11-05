// Typed wrappers around the Settings app's Rust commands.
import { invoke } from '@tauri-apps/api/core';
import { parseStatus, type Status } from './settings';

export interface Info {
  os: string;
  version: string;
  repo: string;
  selfUpdate: boolean;
}

export interface Prefs {
  welcomed: boolean;
  checkUpdates: boolean;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  newer: boolean;
  url: string;
  notes: string | null;
}

export const info = () => invoke<Info>('info');
export const islet = (args: string[], input?: string) => invoke<string>('islet', { args, input: input ?? null });
export const status = async (): Promise<Status> => parseStatus(await islet(['status', '--json']));
export const startIsland = () => invoke<void>('start_island');
export const openLink = (url: string) => invoke<void>('open_link', { url });
export const getPrefs = () => invoke<Prefs>('get_prefs');
export const setPrefs = (prefs: Prefs) => invoke<void>('set_prefs', { prefs });
export const checkUpdate = () => invoke<UpdateInfo>('check_update');
export const installUpdate = () => invoke<void>('install_update');

/** Errors from Rust arrive as strings. */
export const message = (e: unknown) => (typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));
