import { invoke } from '@tauri-apps/api/core';

/** @type {import('./$types').PageLoad} */
export async function load({ }) {
	// Free Mode: no Spotify session, so the Library is just Local Media
	return { freeMode: await invoke("is_controller_mode").catch(() => false) };
}
