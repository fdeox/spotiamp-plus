# Privacy policy

Spotiamp+ has no accounts of its own, no telemetry and no analytics. It doesn't
send information about you or how you use it anywhere.

This program will not transfer any information to other networked systems
unless specifically requested by the user or the person installing or
operating it, except for the following:

- **Spotify**, to log in, play music and fetch song details, covers and lyrics.
  This is what the app is for, and it uses your own Spotify login. Spotify's
  own [privacy policy](https://www.spotify.com/legal/privacy-policy/) applies
  to that connection.
- **GitHub**, when the app starts: it downloads a small file from this
  project's [releases](https://github.com/fdeox/spotiamp-plus/releases) to see
  whether an update is available. Nothing about you is sent with it.
- **The Winamp Skin Museum** (api.webamp.org, skins.webamp.org), only when you
  open the Skin Museum window to browse or put on a skin.
- **Last.fm**, only if you connect your Last.fm account (right-click,
  *Last.fm*): the songs you play (artist, title, album, length and when you
  played them) go to your Last.fm profile. That's what scrobbling is. Last.fm's
  own [privacy policy](https://www.last.fm/legal/privacy) applies. *Disconnect*
  in the same menu stops it.
- **radio-browser.info**, only when you use the Library's *Radio*: what you
  search for, and your country (from Windows' region setting) for the popular
  stations there. When you play a station from it, Spotiamp+ tells the
  directory which station was played, nothing about you, so its popular lists
  stay right.
- **Internet radio stations** you play (from the Library or with Ctrl+L):
  Spotiamp+ connects to that station's own server to play it.

## What stays on your computer

Your settings, playlists, saved lists, loved songs, listening history (used
for the Library's *Recently played* and *Most played* and for *Listening
stats*), the list of your music folders and your radio stations are stored only
on your computer, in `%APPDATA%\spotiamp\config`, and are never uploaded (if
you connect Last.fm, it only gets the plays, as above). The
same folder holds a log file (`spotiamp.log`) that helps with bug reports; it
only leaves your computer if you share it yourself.

Discord Rich Presence talks to the Discord app already running on your
computer; Spotiamp+ doesn't connect to Discord's servers itself.

## Removing it

Spotiamp+ can be uninstalled at any time from Windows' *Installed apps*.
Deleting the `%APPDATA%\spotiamp` folder removes everything it stored.
