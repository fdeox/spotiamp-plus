# Spotiamp+ 0.7.1

A polish and stability update: a classic time toggle, local files that stick around, and a fix for local playback that went silent on some setups.

## New

- **Click the time to switch elapsed / remaining.** Just like classic Winamp, clicking the time readout flips between time played and time left (shown with a leading minus).
- **Local files are remembered across restarts.** Files you add now come back in the playlist after you relaunch, in order, mixed with your Spotify tracks.

## Fixed

- **Local files that played silently and stopped on some setups.** The player was forcing the file's exact sample rate onto your audio device; if the device could not do that rate, the stream failed and the track just stopped with no error. It now plays through whatever rate your device supports (resampling when needed), so local files work on more hardware. Failures are also reported now instead of silently stopping.
- **The equalizer reopens where you left it.** If the EQ was open when you closed the app, it comes back on launch, so a playlist docked below it lines up again instead of floating in the gap.

## Known issues

- **Date is blank for Liked Songs and search results.** A track's added date only comes through for playlists.

Thanks for the reports and suggestions that shaped this one!
