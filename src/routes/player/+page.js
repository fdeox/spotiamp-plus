import { invoke } from '@tauri-apps/api/core';

/** @type {import('./$types').PageLoad} */
export async function load({ }) {
	/**
	 * @type {import('$lib/common.svelte').PlayerSettings}
	 */
	const playerSettings = await invoke("get_player_settings");
	// Controller ("free") mode changes what the transport and ticker talk to,
	// so the page needs to know before first render.
	playerSettings.controller_mode = await invoke("is_controller_mode").catch(() => false);
	return playerSettings;
}