import { invoke } from '@tauri-apps/api/core';

/** @type {import('./$types').PageLoad} */
export async function load({ }) {
	/**
	 * @type {import('$lib/common.svelte').PlaylistSettings}
	 */
	const playlistSettings = await invoke("get_playlist_settings");
	// Controller ("free") mode: the playlist stays empty (tracks would need the
	// librespot session) and the menu hides the session-backed windows.
	playlistSettings.controller_mode = await invoke("is_controller_mode").catch(() => false);
	return playlistSettings;
}